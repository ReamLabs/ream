//! HTTP/RPC surface of the DA node — its only ingress.
//!
//! Because the DA node has no P2P stack, a beacon node drives it entirely
//! through these endpoints: it submits candidate columns to verify and
//! retention hints to prune, and reads availability and stored columns back the
//! same way. The server binds to loopback: it is a local sidecar to one beacon
//! process, not a public network service.

pub mod handlers;
pub mod routes;
pub mod server;

#[cfg(test)]
mod tests;
