//! DA core for `ream da-node`: the storage-and-verification half of a PeerDAS
//! data-availability node.
//!
//! # Division of labor: beacon node vs. DA node
//!
//! A beacon node and a DA node run as two separate OS processes, and the split
//! of responsibility is deliberate:
//!
//! - The **beacon node** owns the consensus business logic — gossip, peer
//!   scoring, fork choice, which blocks are canonical, which columns this node
//!   is custodian of. It drives the DA node over HTTP/RPC.
//! - The **DA node** (this code) only *stores* data columns and *verifies* them
//!   in a self-contained way: SSZ structure, the commitments inclusion proof,
//!   and the KZG cell proofs. It has no P2P stack and no view of the chain — it
//!   cannot tell "canonical" from "orphaned". Everything it needs arrives as an
//!   opaque payload plus a small context across the RPC boundary.
//!
//! This crate is intentionally free of beacon, KZG, and execution
//! dependencies. A concrete proof system enters only through an adapter (see
//! `ream-da-verifier-kzg`) behind the [`verifier::DaVerifier`] trait, so the
//! core stays reusable by a future non-KZG or post-quantum scheme.
pub mod availability;
pub mod column;
pub mod error;
pub mod id;
pub mod store;
pub mod verifier;
