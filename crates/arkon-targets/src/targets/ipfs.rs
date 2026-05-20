use crate::{DeployedUrl, DeployedUrlKind, Target};
use arkon_core::{
    artifact::Artifact,
    config::TargetConfig,
    deploy::DeployCtx,
    error::{ArkonError, Result},
};
use std::path::Path;
use tracing::{debug, info, warn};

pub struct IpfsTarget;

impl Target for IpfsTarget {
    fn name(&self) -> &str { "ipfs" }

    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ArkonError::Other(e.into()))?;
        rt.block_on(self.deploy_async(artifact, ctx, config))
    }
}

impl IpfsTarget {
    async fn deploy_async(
        &self,
        artifact: &Artifact,
        ctx: &DeployCtx,
        config: &TargetConfig,
    ) -> Result<DeployedUrl> {
        let (api_url, pin_service) = extract_ipfs_config(config);

        info!(
            api  = %api_url,
            path = %artifact.path.display(),
            "publishing to IPFS"
        );

        // 1. Add directory to local IPFS node via Kubo HTTP RPC
        let cid = self.ipfs_add_dir(&api_url, &artifact.path).await?;
        info!(cid = %cid, "directory added to IPFS");

        // 2. Pin locally
        self.ipfs_pin(&api_url, &cid).await?;
        info!(cid = %cid, "pinned locally");

        // 3. Remote pin if configured
        if let Some(service) = &pin_service {
            match self.remote_pin(service, &cid, ctx).await {
                Ok(_)  => info!(cid = %cid, service = %service, "remote pin successful"),
                Err(e) => warn!(cid = %cid, error = %e, "remote pin failed (local pin still active)"),
            }
        }

        let gateway_url = format!("https://ipfs.io/ipfs/{cid}");
        let cloudflare_url = format!("https://cloudflare-ipfs.com/ipfs/{cid}");
        info!(ipfs = %gateway_url, cf = %cloudflare_url, "IPFS deploy complete");

        Ok(DeployedUrl {
            url: gateway_url,
            kind: DeployedUrlKind::Ipfs,
        })
    }

    /// POST /api/v0/add?recursive=true&wrap-with-directory=false to local Kubo node.
    /// Returns the root CID.
    async fn ipfs_add_dir(&self, api_url: &str, path: &Path) -> Result<String> {
        // Build multipart form with all files
        let mut form = reqwest::multipart::Form::new();

        for entry in walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let abs  = entry.path().to_path_buf();
            let rel  = abs.strip_prefix(path)
                .map_err(|e| ArkonError::Other(e.into()))?
                .to_string_lossy()
                .replace('\\', "/");

            let bytes = tokio::fs::read(&abs).await?;
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(rel.clone())
                .mime_str("application/octet-stream")
                .map_err(|e| ArkonError::Other(e.into()))?;
            form = form.part(format!("file-{rel}"), part);
            debug!(file = %rel, "staging for IPFS add");
        }

        let client   = reqwest::Client::new();
        let endpoint = format!("{api_url}/api/v0/add?recursive=true&wrap-with-directory=true&quieter=true");

        let resp = client
            .post(&endpoint)
            .multipart(form)
            .send()
            .await
            .map_err(|e| ArkonError::Network(format!("IPFS add: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body   = resp.text().await.unwrap_or_default();
            return Err(ArkonError::Network(format!("IPFS add HTTP {status}: {body}")));
        }

        // Kubo returns NDJSON — each line is a file, last line is the root directory
        let text = resp.text().await
            .map_err(|e| ArkonError::Network(e.to_string()))?;

        let root_cid = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .and_then(|v| v["Hash"].as_str().map(String::from))
            .ok_or_else(|| ArkonError::Network("IPFS add: could not parse root CID".into()))?;

        Ok(root_cid)
    }

    /// POST /api/v0/pin/add?arg=<cid>
    async fn ipfs_pin(&self, api_url: &str, cid: &str) -> Result<()> {
        let client   = reqwest::Client::new();
        let endpoint = format!("{api_url}/api/v0/pin/add?arg={cid}");
        let resp = client.post(&endpoint).send().await
            .map_err(|e| ArkonError::Network(format!("IPFS pin: {e}")))?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ArkonError::Network(format!("IPFS pin failed: {body}")));
        }
        Ok(())
    }

    /// Pin to a remote pinning service (Web3.Storage, Pinata, etc.)
    /// using the IPFS Pinning Services API spec.
    async fn remote_pin(&self, service_url: &str, cid: &str, ctx: &DeployCtx) -> Result<()> {
        let token = ctx.env.get("IPFS_PIN_TOKEN")
            .ok_or_else(|| ArkonError::VaultError(
                "IPFS_PIN_TOKEN not set — add with `arkon secrets set IPFS_PIN_TOKEN`".into()
            ))?;

        let client   = reqwest::Client::new();
        let endpoint = format!("{service_url}/pins");
        let body = serde_json::json!({
            "cid":  cid,
            "name": format!("arkon-{}", chrono::Utc::now().format("%Y%m%d%H%M%S")),
        });

        let resp = client
            .post(&endpoint)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ArkonError::Network(format!("remote pin: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ArkonError::Network(format!("remote pin HTTP error: {body}")));
        }
        Ok(())
    }
}

fn extract_ipfs_config(config: &TargetConfig) -> (String, Option<String>) {
    match config {
        TargetConfig::Ipfs(c) => (
            c.api.clone().unwrap_or_else(|| "http://127.0.0.1:5001".into()),
            c.pin_service.clone(),
        ),
        _ => ("http://127.0.0.1:5001".into(), None),
    }
}
