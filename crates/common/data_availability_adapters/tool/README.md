# ream-data-availability-tool

A small end-to-end driver for the standalone data node. It speaks the same
`/data/v0` RPC the beacon-side feeder will speak, so it doubles as living
documentation of that wire surface — and as a way to exercise the full
ingest → verify → store → availability pipeline against **real mainnet data**
instead of synthetic fixtures.

## Commands

```bash
# Is the data node up?
ream-data-availability-tool health

# What does the data node hold for a block?
ream-data-availability-tool availability <0x-block-root>

# The main event: fetch a block's blobs from a live beacon node, derive its
# 128 data column sidecars locally (real KZG cells + proofs), submit them all,
# and wait until the data node has verified and stored every one.
ream-data-availability-tool feed --beacon-url <beacon-api-url> [block_id]
```

`--da-url` overrides the data node address (default `http://127.0.0.1:5062`).
`block_id` is a slot number, `head`, `finalized`, or a `0x` block root
(default `head`). `--columns 0,5,17` submits a subset; `--wait` bounds the
verification wait in seconds.

## How `feed` derives sidecars without fetching the block

Everything a `DataColumnSidecar` needs is already in the blob sidecars served
by `GET /eth/v1/beacon/blob_sidecars/{block_id}`:

- the **signed block header** (shared by all blobs of the block),
- the **KZG commitments** (one per blob, in index order),
- the **commitments inclusion proof** — the top 4 nodes of each blob's
  depth-17 inclusion branch are exactly the commitments-list → body-root
  segment that the column sidecar needs,
- the **cells and cell proofs**, computed locally from the blob data.

The derived proof is self-checked with `verify_inclusion_proof()` before
anything is submitted, so inconsistent beacon responses fail fast.

## Example walkthrough

```bash
cargo build -p ream -p ream-data-availability-tool

# 1. start the data node (loopback only)
./target/debug/ream -v 4 da_node

# 2. feed it a real block (pick one with blobs)
./target/debug/ream-data-availability-tool feed \
    --beacon-url https://ethereum-beacon-api.publicnode.com finalized
# fetched 7 blob(s) for block finalized, slot 14706334
# derived 128 column sidecars (block root 0xe4bb...a2d5)
# submitted 128 column(s) to the data node
# all submitted columns verified and held:
# { "complete": true, "held_count": 128, "missing": [] }

# 3. ask again any time
./target/debug/ream-data-availability-tool availability 0xe4bb...a2d5
```
