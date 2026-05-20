use arkon_core::{config::{HealthCheck, HealthConfig}, error::Result};
use std::{collections::HashMap, time::Duration};
use tokio::time;
use tracing::{error, info, warn};

/// Background health monitor. Runs as a tokio task.
pub struct HealthMonitor {
    configs: HashMap<String, HealthConfig>,
    client: reqwest::Client,
}

impl HealthMonitor {
    pub fn new(configs: HashMap<String, HealthConfig>) -> Self {
        Self {
            configs,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Run the health monitor loop forever. Call via tokio::spawn.
    pub async fn run(self) {
        info!("health monitor started for {} target(s)", self.configs.len());
        let mut handles = vec![];
        for (target, cfg) in self.configs {
            let client = self.client.clone();
            let cfg = cfg.clone();
            handles.push(tokio::spawn(async move {
                Self::monitor_target(target, cfg, client).await;
            }));
        }
        for h in handles { let _ = h.await; }
    }

    async fn monitor_target(target: String, cfg: HealthConfig, client: reqwest::Client) {
        let interval_secs = parse_duration(&cfg.interval.unwrap_or_else(|| "60s".into()));
        let retries = cfg.retries.unwrap_or(3);
        let webhook = cfg.on_failure.clone();
        let mut ticker = time::interval(Duration::from_secs(interval_secs));

        loop {
            ticker.tick().await;

            let mut all_ok = true;
            for check in &cfg.checks {
                let ok = Self::run_check(check, &client).await;
                if !ok {
                    warn!(target = %target, "health check FAILED: {:?}", check);
                    all_ok = false;
                }
            }

            if !all_ok {
                if let Some(ref url) = webhook {
                    Self::fire_webhook(&client, url, &target).await;
                }
            } else {
                info!(target = %target, "health checks passed");
            }
        }
    }

    async fn run_check(check: &HealthCheck, client: &reqwest::Client) -> bool {
        match check {
            HealthCheck::Http { url, expect } => {
                let expected_status = expect.unwrap_or(200);
                match client.get(url).send().await {
                    Ok(resp) => resp.status().as_u16() == expected_status,
                    Err(e) => {
                        warn!(url = %url, error = %e, "HTTP health check failed");
                        false
                    }
                }
            }
            HealthCheck::Tcp { host, port } => {
                let addr = format!("{host}:{port}");
                tokio::net::TcpStream::connect(&addr).await.is_ok()
            }
            HealthCheck::Process { name } => {
                // Check if process is running via /proc or pgrep
                #[cfg(target_os = "linux")]
                {
                    std::process::Command::new("pgrep")
                        .arg("-x").arg(name)
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    // macOS: use pgrep (available via proctools)
                    // Windows: use tasklist
                    #[cfg(target_os = "macos")]
                    {
                        std::process::Command::new("pgrep")
                            .arg(name)
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false)
                    }
                    #[cfg(target_os = "windows")]
                    {
                        std::process::Command::new("tasklist")
                            .args(["/FI", &format!("IMAGENAME eq {}", name)])
                            .output()
                            .map(|o| String::from_utf8_lossy(&o.stdout).contains(name))
                            .unwrap_or(false)
                    }
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    { true }
                }
            }
        }
    }

    async fn fire_webhook(client: &reqwest::Client, url: &str, target: &str) {
        let body = serde_json::json!({
            "text": format!("⚠️ ARKON health check FAILED for target `{target}`"),
            "target": target,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        match client.post(url).json(&body).send().await {
            Ok(_)  => info!(url = %url, "failure webhook fired"),
            Err(e) => error!(url = %url, error = %e, "failed to fire webhook"),
        }
    }
}

fn parse_duration(s: &str) -> u64 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix('s') { return v.parse().unwrap_or(60); }
    if let Some(v) = s.strip_suffix('m') { return v.parse::<u64>().unwrap_or(1) * 60; }
    if let Some(v) = s.strip_suffix('h') { return v.parse::<u64>().unwrap_or(1) * 3600; }
    60
}
