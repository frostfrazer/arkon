use crate::{DeployedUrl, DeployedUrlKind, Target};
use arkon_core::{
    artifact::Artifact,
    config::TargetConfig,
    deploy::DeployCtx,
    error::{ArkonError, Result},
    runtime::CostHint,
};
use tracing::info;

pub struct WebrtcTarget;

impl Target for WebrtcTarget {
    fn name(&self) -> &str { "webrtc" }

    /// Deploy via WebRTC: start a P2P preview session and return the share link.
    /// This is intentionally a "blocking until Ctrl+C or TTL" operation when called
    /// directly. In daemon mode, the session stays up in the background.
    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let (ttl, _stun) = extract_webrtc_config(config);

        info!(
            project  = %ctx.project_name,
            ttl_secs = %ttl,
            path     = %artifact.path.display(),
            "starting WebRTC P2P preview"
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ArkonError::Other(e.into()))?;

        let (public_url, local_url, peer_id) = rt.block_on(async {
            let session = arkon_p2p::start_preview(
                &artifact.path,
                &ctx.project_name,
                &format_ttl(ttl),
            ).await?;

            let public = session.public_url.clone();
            let local  = session.local_url.clone();
            let peer   = session.peer_id.clone();

            // Keep the session alive in a background task
            // The caller (CLI) handles Ctrl+C separately
            tokio::spawn(async move {
                session.wait_for_expiry().await;
            });

            Ok::<_, ArkonError>((public, local, peer))
        })?;

        info!(
            url    = %public_url,
            peer   = %peer_id,
            "WebRTC session live"
        );

        Ok(DeployedUrl {
            url: public_url.clone(),
            kind: DeployedUrlKind::P2p,
        })
    }

    fn cost_estimate(&self, _artifact: &Artifact, _config: &TargetConfig) -> Option<CostHint> {
        // P2P is always free — data flows peer-to-peer
        Some(CostHint::free())
    }
}

fn extract_webrtc_config(config: &TargetConfig) -> (u64, String) {
    match config {
        TargetConfig::Webrtc(c) => {
            let ttl = c.ttl.as_deref()
                .map(arkon_p2p::parse_ttl)
                .unwrap_or(86_400); // 24h default
            let stun = c.stun.clone()
                .unwrap_or_else(|| "stun.cloudflare.com:3478".into());
            (ttl, stun)
        }
        _ => (86_400, "stun.cloudflare.com:3478".into()),
    }
}

fn format_ttl(secs: u64) -> String {
    if secs % 3600 == 0 { format!("{}h", secs / 3600) }
    else if secs % 60 == 0 { format!("{}m", secs / 60) }
    else { format!("{secs}s") }
}
