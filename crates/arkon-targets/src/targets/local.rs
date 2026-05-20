use crate::{DeployedUrl, Target};
use arkon_core::{
    artifact::Artifact,
    config::{LocalTarget as LocalConfig, TargetConfig},
    deploy::DeployCtx,
    error::{ArkonError, Result},
};
use tracing::info;

pub struct LocalTarget;

impl Target for LocalTarget {
    fn name(&self) -> &str { "local" }

    fn deploy(&self, artifact: &Artifact, _ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let cfg = local_cfg(config)?;
        let dest = &cfg.path;
        std::fs::create_dir_all(dest)?;

        info!(
            src  = %artifact.path.display(),
            dest = %dest.display(),
            "local copy deploy"
        );

        // Recursively copy artifact output to destination
        copy_dir(&artifact.path, dest)?;

        Ok(DeployedUrl::local(dest.to_string_lossy().into_owned()))
    }
}

fn local_cfg(config: &TargetConfig) -> Result<&LocalConfig> {
    match config {
        TargetConfig::Local(c) => Ok(c),
        _ => Err(ArkonError::ConfigError("expected local target config".into())),
    }
}

fn copy_dir(src: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).unwrap();
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
