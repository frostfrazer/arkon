//! Local network discovery via mDNS (DNS-SD / Bonjour / Avahi).
//!
//! Service type: `_arkon._tcp.local.`
//! TXT records:
//!   - `game=<name>`     game or project name
//!   - `version=<ver>`   game version
//!   - `peer=<peer_id>`  ARKON peer ID
//!   - `port=<port>`     local HTTP server port
//!   - `type=preview|game|download`  what's being served

use arkon_core::error::{ArkonError, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{
    collections::HashMap,
    net::Ipv4Addr,
    time::Duration,
};
use tokio::time;
use tracing::{debug, info, warn};

const SERVICE_TYPE: &str = "_arkon._tcp.local.";

/// A discovered ARKON peer on the local network.
#[derive(Debug, Clone)]
pub struct LanPeer {
    pub name:     String,
    pub game:     String,
    pub version:  String,
    pub peer_id:  String,
    pub host:     String,
    pub port:     u16,
    pub kind:     String,
}

impl LanPeer {
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// Broadcasts an ARKON service on the local network via mDNS.
pub struct LanBroadcaster {
    daemon:       ServiceDaemon,
    service_name: String,
}

impl LanBroadcaster {
    /// Start broadcasting a service on the LAN.
    ///
    /// # Arguments
    /// * `name`      — project / game name
    /// * `version`   — version string
    /// * `port`      — local HTTP server port
    /// * `kind`      — "preview", "game", or "download"
    /// * `peer_id`   — ARKON peer ID (from DhtNode or PeerIdentity)
    pub fn start(
        name:    &str,
        version: &str,
        port:    u16,
        kind:    &str,
        peer_id: &str,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("mDNS daemon: {e}")))?;

        // Build TXT record
        let mut properties: HashMap<String, String> = HashMap::new();
        properties.insert("game".into(),    name.to_string());
        properties.insert("version".into(), version.to_string());
        properties.insert("peer".into(),    peer_id.to_string());
        properties.insert("type".into(),    kind.to_string());

        // Use machine hostname; fall back to "arkon-host"
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "arkon-host".into());

        // Instance name: "<name>._arkon._tcp.local."
        let instance_name = format!("{}-{}", sanitize(name), &peer_id[..8]);

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &format!("{hostname}.local."),
            "",   // IP resolved automatically
            port,
            properties,
        )
        .map_err(|e| ArkonError::Other(anyhow::anyhow!("ServiceInfo: {e}")))?;

        daemon.register(service)
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("mDNS register: {e}")))?;

        info!(
            name    = %name,
            port    = %port,
            kind    = %kind,
            service = %SERVICE_TYPE,
            "mDNS service registered — visible on local network"
        );

        Ok(Self {
            daemon,
            service_name: instance_name,
        })
    }

    /// Stop broadcasting. The service is unregistered from the network.
    pub fn stop(self) {
        if let Err(e) = self.daemon.unregister(&self.service_name) {
            warn!(error = %e, "mDNS unregister failed");
        }
        let _ = self.daemon.shutdown();
        info!("mDNS service stopped");
    }
}

/// Scans the local network for ARKON services via mDNS.
pub struct LanScanner {
    daemon: ServiceDaemon,
}

impl LanScanner {
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("mDNS daemon: {e}")))?;
        Ok(Self { daemon })
    }

    /// Discover all ARKON peers on the local network.
    /// Listens for `timeout` and returns everything found.
    pub async fn scan(&self, timeout: Duration) -> Result<Vec<LanPeer>> {
        let receiver = self.daemon.browse(SERVICE_TYPE)
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("mDNS browse: {e}")))?;

        let mut peers: Vec<LanPeer> = Vec::new();
        let deadline = time::Instant::now() + timeout;

        loop {
            tokio::select! {
                _ = time::sleep_until(deadline) => break,
                event = tokio::task::spawn_blocking({
                    let rx = receiver.clone();
                    move || rx.recv_timeout(Duration::from_millis(100))
                }) => {
                    match event {
                        Ok(Ok(ServiceEvent::ServiceResolved(info))) => {
                            if let Some(peer) = Self::parse_service(&info) {
                                info!(
                                    name = %peer.name,
                                    url  = %peer.url(),
                                    "discovered LAN peer"
                                );
                                peers.push(peer);
                            }
                        }
                        Ok(Ok(ServiceEvent::ServiceRemoved(_, name))) => {
                            debug!(name = %name, "LAN peer left");
                            peers.retain(|p| !p.peer_id.starts_with(&name));
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(peers)
    }

    fn parse_service(info: &ServiceInfo) -> Option<LanPeer> {
        let props  = info.get_properties();
        let game   = props.get("game")?.val_str().to_string();
        let version= props.get("version").map(|v| v.val_str().to_string())
            .unwrap_or_else(|| "unknown".into());
        let peer_id= props.get("peer")?.val_str().to_string();
        let kind   = props.get("type").map(|v| v.val_str().to_string())
            .unwrap_or_else(|| "preview".into());

        let host   = info.get_hostname().trim_end_matches('.').to_string();
        let port   = info.get_port();
        let name   = info.get_fullname().to_string();

        Some(LanPeer { name, game, version, peer_id, host, port, kind })
    }
}

impl Default for LanScanner {
    fn default() -> Self {
        Self::new().expect("mDNS scanner init")
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .to_lowercase()
}

// hostname crate needed — add to p2p Cargo.toml
