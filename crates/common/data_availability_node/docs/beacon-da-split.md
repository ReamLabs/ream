# Beacon node ↔ DA node: the split, the seam, and the feeder checklist

Context for reviewers of the standalone DA node. A beacon node and a DA node run
as **two separate OS processes**. This note records how responsibility is
partitioned, why column verification lives in the DA node (not only in gossip),
and exactly which beacon-side touch points must move to RPC so the column
dataset stays single-owned.

## Two datasets, two owners — a partition, not a copy

| Dataset | Owner | The other side |
| --- | --- | --- |
| Consensus data — blocks, states, checkpoints, slot clock, proposer schedule | **beacon** | DA never has it and never needs it (it has no chain view) |
| Column payloads — the `DataColumnSidecar` bytes we durably store and serve | **DA node** | beacon must **not** persist these; it forwards them and reads them back over RPC |

Gossip anti-spam metadata (`seen_data_column_sidecars`) is beacon-local and is
*not* the column dataset. Consensus reads during gossip validation are beacon
reading its **own** data — they do not fragment the DA dataset.

The only way the column dataset fragments is if beacon keeps persisting columns
(see SEAM ① below) *and* the DA node stores them too. Moving all four touch
points to RPC keeps a single owner.

## Two-tier verification (why the DA node is not "just storage")

- **Gossip validation** (`validate_data_column_sidecar_full`, called
  synchronously from `handle_gossipsub_message`) is a **propagation gate**. Its
  KZG check is a spec-forced `[REJECT]` condition, so it must stay inline in the
  beacon — it cannot be deferred to an async DA round-trip without coupling mesh
  speed to DA throughput.
- **DA verification** is a **storage-admission gate** for the custody dataset.
  Gossip is only one ingress path; **req/resp backfill, DAS sampling, and
  reconstruction never pass through gossip validation**, so for those paths the
  DA node's verification is the *only* verification. The RPC ingest endpoint is
  deliberately source-agnostic so every source funnels through one
  verify → store gate (a self-certifying store). The gossip-path double-verify
  is the accepted cost of the process split.

Open decision for the team: **does beacon forward non-gossip columns raw or
pre-verified?** Raw ⇒ DA is the verification+storage authority (current design);
pre-verified everywhere ⇒ DA collapses to storage+serving.

## The seam: 4 beacon touch points → DA RPC

Every beacon-side use of the column store maps 1:1 onto an endpoint the DA node
already exposes.

```
                    BEACON NODE  (owns consensus data + all P2P)
  ┌──────────────────────────────────────────────────────────────────────┐
  │  gossip mesh ─► handle_gossipsub_message                               │
  │                  └─ validate_data_column_sidecar_full  (sync)          │
  │                       • reads consensus data (slot/finalized/head/     │
  │                         state/parent/checkpoint)  ── beacon's own      │
  │                       • KZG = PROPAGATION gate (spec [REJECT])         │
  │                       • Accept ─► re-gossip                            │
  │                               └─► SEAM ① forward payload ─────────────┐│
  │                                                                       ││
  │  req/resp peer ◄─ DataColumnSidecarsByRoot / ByRange                  ││
  │                     └─ SEAM ②③ needs column payload ──────────────────┤│
  │                                                                       ││
  │  fork choice ── is_data_available? ── SEAM ④ needs availability ──────┤│
  └───────────────────────────────────────────────────────────────────────┘
                                   │  HTTP/RPC (loopback only)
                                   ▼
                    DA NODE  (sole owner of column payloads)
  ┌──────────────────────────────────────────────────────────────────────┐
  │  POST  ingest             ◄── ①  candidate payload → verify → store    │
  │  GET   /columns/{root}          ②  whole-block columns                 │
  │  GET   /columns/{root}/{index}  ③  one stored column payload           │
  │  GET   /availability/{root}     ④  held / complete / missing           │
  └──────────────────────────────────────────────────────────────────────┘
```

| Seam | Beacon site today | Kind | Becomes | DA endpoint (exists) |
| --- | --- | --- | --- | --- |
| ① | `gossipsub/handle.rs:494` — insert on gossip Accept | write | forward to DA (feeder) | `POST` ingest |
| ② | `req_resp.rs:244` — serve `…ByRoot` | read | fetch from DA, then answer peer | `GET /columns/{root}/{index}` |
| ③ | `req_resp.rs:271` — serve `…ByRange` | read | fetch from DA, then answer peer | `GET /columns/{root}` |
| ④ | `fork_choice/beacon/src/store.rs:766` — availability read | read | ask DA | `GET /availability/{root}` |

(Line numbers are as of this note; verify against current code before editing.)

## Feeder implementation checklist

The feeder is the beacon-side glue that drives the DA node. Building it is the
same act as removing beacon's column persistence — do both together.

- [ ] **SEAM ① — stop persisting, start forwarding.** Replace the
      `column_sidecars_provider().insert(...)` on gossip Accept with a
      fire-and-forget POST to the DA ingest endpoint. Keep the `send_gossip`
      re-propagation. The forward must be **non-blocking and honor 503**
      (`Overloaded`) — it must never block the gossip handler, or DA throughput
      would backpressure the mesh.
- [ ] **SEAM ②③ — serve req/resp from DA.** Before answering a peer's
      `DataColumnSidecarsByRoot/ByRange`, fetch the column(s) from the DA node
      over RPC instead of the local `column_sidecars_provider`.
- [ ] **SEAM ④ — fork-choice availability via DA.** Replace the fork-choice
      column read with a `GET /availability/{root}` call to the DA node.
- [ ] **Retention.** On finalization, beacon issues a retention hint to the DA
      node (endpoint already present) so pruning is beacon-driven.
- [ ] **Decommission the beacon column store.** Once all four seams are moved,
      remove `column_sidecars_provider` from the beacon DB so there is exactly
      one column store.
- [ ] **Decouple.** Beacon → DA must be async with load-shedding at every seam;
      no synchronous wait on DA inside the gossip handler.

## Not-yet-wired PeerDAS in beacon — the DA node's forward TODO

The seams above cover what beacon *already* does with columns. Everything else
the Fulu spec requires exists in the beacon tree today **only as types and
primitives with no callers** — verified by grep, zero call sites outside their
own module. Each one, once beacon wires it, drives either a new DA endpoint or
an architecture decision on our side. Tracked here so the DA RPC surface is
planned ahead instead of retrofitted.

What is wired today: gossip receive → validate → store → serve peers (req/resp)
→ fork-choice DA gate. That is the whole of it. The features below are not.

**Interim posture.** The beacon side is moving slower than the DA node, so the
seams above are not yet cut over: beacon still persists columns and still
re-verifies them, so the duplication this note describes persists for now. That
is accepted on purpose. The DA client proceeds on its own design and plan and
does **not** block on beacon; each duplication is resolved when beacon moves the
corresponding seam, not before.

| Unwired beacon feature | State today (no callers) | Drives on the DA side |
| --- | --- | --- |
| Reconstruction | `recover_matrix`, `recover_cells_and_kzg_proofs` in `consensus/beacon/src/matrix_entry.rs` | ownership decision: compute in DA vs beacon |
| Backfill / sync writes | only the gossip path calls `insert`; the syncer never stores fetched columns | **batch ingest** endpoint |
| Custody / DAS sampling | the three `consensus/beacon/src/custody_group.rs` fns have zero callers | availability relative to a custody set, not all 128 |
| Column HTTP API | beacon serves `blob_sidecars` over HTTP (`rpc/beacon/src/handlers/blob_sidecar.rs`), no column equivalent | `GET /columns` must back it; batch read |
| Column pruning | blobs prune by retain-set (`storage/.../blobs_and_proofs.rs`); columns only have per-key `remove` | own the by-slot prune behind `/retention/{slot}` |

Checklist — settle the decision first, then build the endpoint:

- [x] **Reconstruction ownership — DECIDED: the DA node.** Erasure/KZG recovery
      (`recover_cells_and_kzg_proofs`, `recover_matrix`) lives in the DA node's
      KZG adapter, not beacon. Rationale:
      1. **Data locality** — the ≥50% of columns recovery needs already live in
         the DA node. Reconstructing in beacon would ship that data across the
         process boundary and push the result back; reconstructing in DA is
         in-place.
      2. **KZG is already there** — recovery needs a KZG context, which the
         `ream-da-verifier-kzg` adapter already holds. The `ream-da` core stays
         KZG-free; recovery lives in the adapter layer, consistent with the
         dependency rule.
      3. **Self-certifying store** — recovered columns never passed
         gossip/beacon verification, so they must be verified before storage.
         Routing them through the DA ingest verify → store gate is the natural
         fit and covers them uniformly with every other source.

      Remaining work / one nuance:
      - [ ] Add recovery to the adapter and re-admit recovered columns through
            the same verify → store gate as any other ingest.
      - [ ] Trigger is **DA-local**: fire when a block is held ≥50% and
            incomplete (bound by slot recency to avoid wasted work). Recovery
            restores all 128 columns regardless of custody; custody only governs
            which columns beacon later asks the DA node to retain/serve, so it
            does not gate the trigger. (The trigger is the only faintly
            consensus-flavored part, and this keeps it out of beacon.)
- [ ] **Batch ingest (endpoint).** `POST /ingest` takes one column. Backfill,
      reconstruction, and block-building all produce columns in bulk (up to 128
      per block); one HTTP round-trip per column will not keep up. Add a
      batch-accepting ingest that still funnels every column through the same
      verify → store gate.
- [ ] **Batch / multi-root read (endpoint).** A req/resp `…ByRoot` request can
      bundle many `(root, indices)` pairs, and `…ByRange` expands to many roots
      after beacon resolves slot→root on its own side. `GET /columns/{root}`
      answers one root per call; add a batch read so serving a peer is not N
      round-trips.
- [ ] **Custody-aware availability (endpoint semantics).** Once beacon wires
      `custody_group`, "missing" must be computed relative to the node's custody
      set, not all 128 columns. Let `/availability` take (or be told) the
      expected column set so `complete`/`missing` mean the right thing.
- [ ] **Column HTTP API backing (follow beacon).** When beacon adds a
      data-column HTTP endpoint (mirroring `blob_sidecars`, likely
      `/eth/v1/debug/beacon/data_column_sidecars/{block_id}`), it will read from
      the DA node. Confirm `GET /columns` (+ the batch read above) can back it.
- [ ] **Pruning semantics — DECIDED (2026-07-07): retention hints are a
      persistent floor (watermark), not one-shot GC.** The boundary is
      monotonic (`max(current, new)`), survives restarts, and gates writes:
      below-watermark candidates are refused. No backfill conflict: the beacon
      computes the boundary as the serving-window edge, so anything it still
      wants is above the floor by construction. Layering: the verifier never
      sees retention (policy, not validity); the store enforces it in `put`
      (new `InsertOutcome` variant) and filters during the startup scan; the
      service adds a cheap early skip. Persist as a tiny file (tmp+rename),
      written **before** pruning so a crash re-prunes idempotently. Remaining:
      implement; keep persistence minimal since the file store → DB migration
      will carry the watermark into the schema.
- [ ] **Store concurrency contract — single-writer is currently external;
      the DB store should own it (decided direction, 2026-07-13).** Today the
      store is only safe because the verification service is the sole writer
      (queue-serialized). Under concurrent writers two real races exist:
      put↔prune TOCTOU (floor checked, prune interleaves, a below-floor column
      lands and is served until the next startup re-prune) and put↔put on the
      same id (same tmp path, interleaved file writes). The file store cannot
      fix this cheaply — filesystems offer atomic single ops (rename) but no
      multi-file transactions — so do NOT retrofit it; when the DB store lands
      (after batch read), write its contract as **every op atomic, concurrent
      callers safe** (put = one txn: floor check + unique insert; prune = floor
      update + range delete in one txn) and pin the contract with concurrent
      put↔prune tests, not just docs. Payoff once that holds: the ingest queue
      demotes from correctness wall to pure backpressure; reconstruction
      becomes a self-contained task (sleep → recheck → recover → verify → put)
      and the service's WeakSender self-handle disappears; the put-gate
      `BelowRetention` arm becomes the live first defense it was built to be.
- [ ] **DA gate trust boundary — Design A vs B.** The core question: where does
      the *authoritative* column verification happen — beacon or DA?
      - **Design A (verify-at-edge).** Every ingress path verifies before the DA
        node. DA is a dumb store with no KZG dependency. `/availability` "held"
        means only "present"; validity is guaranteed upstream, so beacon must
        ensure every path verified.
      - **Design B (DA authority — current).** DA verifies every column on
        ingest (self-certifying store); beacon forwards raw. `/availability`
        "held" means "present **and** valid", so beacon can drop its own
        re-verify in `is_data_available`.

      Choosing reconstruction-in-DA (above) forces KZG into the DA node, which
      rules out Design A — **we are committed to B.** The only remaining action
      is on the beacon side: drop the KZG re-verify in `is_data_available` and
      trust `/availability`. Until beacon does, the system runs a harmless B+A
      hybrid (double-verify), which is fine in the interim (see Interim posture).
