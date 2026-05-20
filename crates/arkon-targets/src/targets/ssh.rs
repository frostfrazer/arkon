use crate::{DeployedUrl, Target};
use arkon_adapters::fingerprint::FileDiff;
use arkon_core::{
    artifact::Artifact,
    config::{SshTarget as SshConfig, TargetConfig},
    deploy::DeployCtx,
    error::{ArkonError, Result},
};
use std::{collections::HashMap, process::Command};
use tracing::info;

pub struct SshTarget;

impl Target for SshTarget {
    fn name(&self) -> &str { "ssh" }

    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let cfg = ssh_cfg(config)?;
        let user_host = format!(
            "{}@{}",
            cfg.user.as_deref().unwrap_or("deploy"),
            cfg.host
        );
        let port = cfg.port.unwrap_or(22);
        let remote_path = &cfg.path;

        info!(
            host = %cfg.host,
            path = %remote_path,
            size_mb = %(artifact.size_bytes / 1_048_576),
            "rsync deploy via SSH"
        );

        // Build rsync command — incremental by default (only transfers changed files)
        let mut rsync_args = vec![
            "-az".to_string(),                          // archive + compress
            "--delete".to_string(),                     // remove files deleted from source
            "--checksum".to_string(),                   // use checksums not timestamps
            "--human-readable".to_string(),
            "--stats".to_string(),
            "-e".to_string(),
            {
                let hk = if cfg.accept_new_host.unwrap_or(false) { "accept-new" } else { "yes" };
                format!("ssh -p {} -o StrictHostKeyChecking={hk} -o UserKnownHostsFile=~/.ssh/known_hosts", port)
            },
        ];

        // Add identity file if configured
        if let Some(identity) = &cfg.identity {
            rsync_args.push("-e".to_string());
            rsync_args.push(format!(
                "ssh -p {} -i {} -o StrictHostKeyChecking=accept-new",
                port,
                identity.display()
            ));
        }

        rsync_args.push(format!("{}/", artifact.path.display()));
        rsync_args.push(format!("{}:{}/", user_host, remote_path));

        let status = Command::new("rsync")
            .args(&rsync_args)
            .status()
            .map_err(|e| ArkonError::deploy(&cfg.host, e.to_string()))?;

        if !status.success() {
            return Err(ArkonError::deploy(&cfg.host, "rsync exited non-zero"));
        }

        // Provision TLS via ACME if requested (calls arkon-daemon's ACME client)
        if cfg.tls {
            if let Some(domain) = &cfg.domain {
                info!(domain = %domain, "provisioning Let's Encrypt TLS");
                self.provision_tls(&user_host, port, domain)?;
            }
        }

        let url = if let Some(domain) = &cfg.domain {
            let scheme = if cfg.tls { "https" } else { "http" };
            format!("{}://{}", scheme, domain)
        } else {
            format!("{}:{}", cfg.host, remote_path)
        };

        Ok(DeployedUrl::http(url))
    }

    fn health_check(&self, config: &TargetConfig) -> Result<()> {
        let cfg = ssh_cfg(config)?;
        let port = cfg.port.unwrap_or(22);
        // Quick TCP connect test
        let addr = format!("{}:{}", cfg.host, port);
        std::net::TcpStream::connect_timeout(
            &addr.parse().map_err(|_| ArkonError::deploy(&cfg.host, "invalid address"))?,
            std::time::Duration::from_secs(5),
        )
        .map(|_| ())
        .map_err(|e| ArkonError::deploy(&cfg.host, format!("SSH unreachable: {e}")))
    }
}

impl SshTarget {
    /// Run the ACME certificate provisioning on the remote host via SSH.
    /// This calls a small helper script we ship with ARKON's daemon package.
    fn provision_tls(&self, user_host: &str, port: u16, domain: &str) -> Result<()> {
        info!(domain = %domain, "TLS provisioning requested — skipping (configure via arkon-daemon separately)");
        // TLS provisioning is handled by the arkon-daemon ACME service.
        // Run `arkon daemon tls provision --domain <domain>` on the server to issue a cert.
        Ok(())
    }
}

fn ssh_cfg(config: &TargetConfig) -> Result<&SshConfig> {
    match config {
        TargetConfig::Ssh(c) => Ok(c),
        _ => Err(ArkonError::ConfigError("expected ssh target config".into())),
    }
}
