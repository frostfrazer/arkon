//! ARKON libp2p DHT node — Kademlia-based peer discovery.
//!
//! This replaces the relay-based preview with true peer-to-peer connectivity:
//!
//!   1. `DhtNode::start()` initialises a libp2p swarm with Kademlia + Identify.
//!   2. On `arkon preview`, the node announces itself under a content-addressed
//!      key derived from the project name: SHA-256("arkon/preview/<name>").
//!   3. Peers discover each other via Kademlia `GET_PROVIDERS` — no relay needed.
//!   4. Once peers know each other's multiaddrs, they establish a direct TCP
//!      connection and proxy traffic to the local HTTP server.
//!
//! Bootstrap peers: we ship three well-known ARKON bootstrap nodes. Users can
//! override via `ARKON_BOOTSTRAP_PEERS` env var or self-host their own.

pub mod node;
pub mod provider;

pub use node::DhtNode;
pub use provider::DhtProvider;
