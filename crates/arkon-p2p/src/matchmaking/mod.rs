//! ARKON P2P matchmaking and local WiFi discovery.
//!
//! Two distinct use cases:
//!
//! ## 1. Internet matchmaking (DHT-based)
//! Game servers register in the Kademlia DHT under a content key derived from
//! the game name + version. Players query the DHT to find live servers, then
//! connect directly (TCP hole-punching where possible, relay otherwise).
//!
//! ## 2. Local WiFi / LAN discovery (mDNS)
//! On the same LAN, peers discover each other via DNS-SD (Bonjour/Avahi).
//! `arkon serve --local` broadcasts a `_arkon._tcp` service record.
//! Other machines on the same network find it with `arkon scan`.
//! Completely offline — no internet required.

pub mod lan;
pub mod internet;

pub use lan::{LanBroadcaster, LanScanner};
pub use internet::InternetMatchmaker;
