use crate::{Target, DeployedUrl, targets::{
    ssh::SshTarget,
    s3::S3Target,
    local::LocalTarget,
    docker::DockerTarget,
    ipfs::IpfsTarget,
    github_pages::GithubPagesTarget,
    torrent::TorrentTarget,
    webrtc::WebrtcTarget,
}};
use arkon_core::{
    artifact::Artifact,
    config::{ArkonConfig, HooksConfig, TargetConfig},
    deploy::{DeployCtx, DeployRecord, DeployStatus},
    error::{ArkonError, Result},
    runtime::CostHint,
};
use arkon_adapters::runner::run_hook;
use chrono::Utc;
use std::collections::HashMap;
use tracing::{info, warn};

pub struct Dispatcher {
    targets: HashMap<String, Box<dyn Target>>,
}

impl Default for Dispatcher {
    fn default() -> Self {
        let mut d = Self { targets: HashMap::new() };
        d.register(SshTarget);
        d.register(S3Target);
        d.register(LocalTarget);
        d.register(DockerTarget);
        d.register(IpfsTarget);
        d.register(GithubPagesTarget);
        d.register(TorrentTarget);
        d.register(WebrtcTarget);
        d
    }
}

impl Dispatcher {
    pub fn register(&mut self, target: impl Target + 'static) {
        self.targets.insert(target.name().to_string(), Box::new(target));
    }

    /// Full deploy pipeline for one target:
    ///   pre-flight → cost check → pre-hooks → deploy → post-hooks → record
    pub fn dispatch(
        &self,
        artifact: &Artifact,
        target_name: &str,
        ctx: &DeployCtx,
        config: &ArkonConfig,
    ) -> Result<DeployRecord> {
        // Dry-run: validate everything but skip actual deploy + hooks
        if ctx.dry_run {
            let target_cfg = config.target(target_name)?;
            let target_impl = self.resolve_target(target_cfg)?;
            info!(target = %target_name, "dry run — skipping deploy");
            if let Some(hint) = target_impl.cost_estimate(artifact, target_cfg) {
                info!(
                    estimate_usd = %format!("{:.4}", hint.total()),
                    breakdown    = %hint.breakdown,
                    "cost estimate (dry run)"
                );
            }
            // Return a synthetic record so callers don't need special-case logic
            let mut record = DeployRecord::new(
                &config.project.name, target_name,
                "dry-run", &artifact.fingerprint,
            );
            record.status      = arkon_core::deploy::DeployStatus::Skipped;
            record.size_bytes  = artifact.size_bytes;
            record.notes       = Some(format!("[dry-run] {target_name}"));
            return Ok(record);
        }

        let target_cfg = config.target(target_name)?;
        let target_impl = self.resolve_target(target_cfg)?;

        // 1. Pre-flight health check
        info!(target = %target_name, "running pre-flight health check");
        target_impl.health_check(target_cfg)?;

        // 2. Cost estimate + optional prompt
        if config.deploy.confirm_cost {
            if let Some(hint) = target_impl.cost_estimate(artifact, target_cfg) {
                let threshold = config.deploy.cost_threshold_usd.unwrap_or(0.01);
                if hint.total() > threshold {
                    // In real CLI this prompts interactively; here we just log
                    warn!(
                        estimate_usd = %format!("{:.4}", hint.total()),
                        breakdown = %hint.breakdown,
                        "cost estimate"
                    );
                }
            }
        }

        // 3. Pre-deploy hooks
        let hooks = config.hooks.get(target_name).cloned().unwrap_or_default();
        self.run_hooks(&hooks.pre_deploy, ctx)?;

        // 4. Deploy
        let started = Utc::now();
        info!(target = %target_name, artifact = %artifact.name, "deploying");
        let deployed_url = target_impl.deploy(artifact, ctx, target_cfg)?;
        let finished = Utc::now();

        info!(
            target = %target_name,
            url = %deployed_url.url,
            duration_ms = %(finished - started).num_milliseconds(),
            "deploy complete"
        );

        // 5. Post-deploy hooks
        self.run_hooks(&hooks.post_deploy, ctx)?;

        // 6. Build and return the deploy record (written to audit log by daemon)
        let duration_ms = (finished - started).num_milliseconds().max(0) as u64;
        let mut record = DeployRecord::new(
            &ctx.project_name,
            target_name,
            "unknown", // filled in by caller who knows the adapter name
            &artifact.fingerprint,
        );
        record.started_at  = started;
        record.finished_at = finished;
        record.duration_ms = duration_ms;
        record.size_bytes  = artifact.size_bytes;
        record.status      = DeployStatus::Success;
        record.notes       = Some(deployed_url.url);

        Ok(record)
    }

    fn resolve_target(&self, config: &TargetConfig) -> Result<&dyn Target> {
        let kind = match config {
            TargetConfig::Ssh(_)          => "ssh",
            TargetConfig::S3(_)           => "s3",
            TargetConfig::B2(_)           => "s3",
            TargetConfig::R2(_)           => "s3",
            TargetConfig::Local(_)        => "local",
            TargetConfig::Webrtc(_)       => "webrtc",
            TargetConfig::Ipfs(_)         => "ipfs",
            TargetConfig::GithubPages(_)  => "github-pages",
        };
        self.targets.get(kind)
            .map(|t| t.as_ref())
            .ok_or_else(|| ArkonError::TargetNotFound { name: kind.to_string() })
    }

    /// Public accessor — returns a target impl for a given config, without deploying.
    /// Used by `arkon cost` to call `cost_estimate` without side effects.
    pub fn target_for(&self, config: &TargetConfig) -> Option<&dyn Target> {
        self.resolve_target(config).ok()
    }

    fn run_hooks(&self, hooks: &[String], ctx: &DeployCtx) -> Result<()> {
        for hook in hooks {
            run_hook(hook, &ctx.project_root, &ctx.env)?;
        }
        Ok(())
    }
}
