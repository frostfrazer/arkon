use arkon_core::error::{ArkonError, Result};
use std::{
    collections::HashMap,
    path::Path,
    process::{Command, Stdio},
};
use tracing::{debug, info};

/// Execute a shell command in `cwd` with extra `env` vars merged into the process env.
/// Streams stdout/stderr to the terminal in real time.
/// Returns an error if the exit code is non-zero.
pub fn run_shell_command(
    cmd: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<()> {
    info!(cmd = %cmd, cwd = %cwd.display(), "running build command");

    // Split on first space for the binary; pass rest as shell -c for portability
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .envs(env)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| ArkonError::BuildFailed(format!("failed to spawn '{cmd}': {e}")))?;

    let status = child
        .wait()
        .map_err(|e| ArkonError::BuildFailed(e.to_string()))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(ArkonError::BuildFailed(format!(
            "command '{cmd}' exited with code {code}"
        )));
    }
    Ok(())
}

/// Run a build hook (pre or post). Same as run_shell_command but with hook-specific error.
pub fn run_hook(hook: &str, cwd: &Path, env: &HashMap<String, String>) -> Result<()> {
    info!(hook = %hook, "running hook");
    run_shell_command(hook, cwd, env).map_err(|_| ArkonError::HookFailed {
        hook: hook.to_string(),
        code: -1,
    })
}

/// Orchestrates a full adapter build cycle:
///   1. Check build cache (skip if inputs unchanged)
///   2. Run pre_build hook
///   3. Invoke adapter.build()
///   4. Fingerprint the output directory
///   5. Run post_build hook
///   6. Return the Artifact
pub struct BuildRunner<'a> {
    pub adapter: &'a dyn crate::Adapter,
    pub ctx: &'a crate::BuildCtx,
}

impl<'a> BuildRunner<'a> {
    pub fn run(&self) -> Result<arkon_core::artifact::Artifact> {
        let cache_key = self.adapter.cache_key(self.ctx);
        debug!(cache_key = %cache_key, "computed build cache key");

        // Cache hit: skip build entirely
        if self.ctx.cache_enabled && cache_key == self.ctx.last_fingerprint {
            info!("build cache HIT — skipping rebuild");
            // Return a synthetic artifact pointing at the cached output dir
            let output = self.adapter.output_dir(self.ctx);
            let (fingerprint, file_hashes, size_bytes) =
                crate::fingerprint::fingerprint_dir(&output)?;
            let mut artifact = arkon_core::artifact::Artifact::new(
                self.adapter.name(),
                output,
                self.adapter.deployable_type(),
            );
            artifact.fingerprint = fingerprint;
            artifact.file_hashes = file_hashes;
            artifact.size_bytes = size_bytes;
            return Ok(artifact);
        }

        info!(adapter = %self.adapter.name(), "build cache MISS — building");

        self.adapter.pre_build(self.ctx)?;
        let mut artifact = self.adapter.build(self.ctx)?;

        // Fingerprint output if adapter didn't already
        if artifact.fingerprint.is_empty() {
            let (fingerprint, file_hashes, size_bytes) =
                crate::fingerprint::fingerprint_dir(&artifact.path)?;
            artifact.fingerprint = fingerprint;
            artifact.file_hashes = file_hashes;
            artifact.size_bytes = size_bytes;
        }

        self.adapter.post_build(&artifact)?;
        info!(
            fingerprint = %&artifact.fingerprint[..12],
            size_mb = %(artifact.size_bytes / 1_048_576),
            "build complete"
        );
        Ok(artifact)
    }
}
