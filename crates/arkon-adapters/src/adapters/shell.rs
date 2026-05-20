use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};
use std::path::PathBuf;

pub struct ShellAdapter;

impl Adapter for ShellAdapter {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str { "Generic shell build — runs build.sh and deploys output" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        let cmd = ctx.command_override.as_deref().unwrap_or("bash build.sh");
        runner::run_shell_command(cmd, &ctx.root, &ctx.env)?;

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("shell", output, DeployableKind::Static);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        for candidate in &["dist", "out", "build", "output", "public"] {
            let p = ctx.root.join(candidate);
            if p.exists() { return p; }
        }
        ctx.root.clone()
    }

    fn runtime_info(&self) -> Runtime { Runtime::native() }
    fn deployable_type(&self) -> DeployableKind { DeployableKind::Static }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(&ctx.root, &["build.sh"])
    }
}
