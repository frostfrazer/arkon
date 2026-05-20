use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};
use std::path::PathBuf;

pub struct ViteAdapter;

impl Adapter for ViteAdapter {
    fn name(&self) -> &str { "vite" }
    fn description(&self) -> &str { "Vite project — React, Vue, Svelte, Solid" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        let cmd = ctx.command_override.as_deref().unwrap_or("npm run build");
        runner::run_shell_command(cmd, &ctx.root, &ctx.env)?;

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("vite", output, DeployableKind::Static);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        ctx.root.join("dist")
    }

    fn runtime_info(&self) -> Runtime { Runtime::static_files() }
    fn deployable_type(&self) -> DeployableKind { DeployableKind::Static }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(
            &ctx.root,
            &["package-lock.json", "yarn.lock", "pnpm-lock.yaml",
              "vite.config.js", "vite.config.ts", "vite.config.mjs"],
        )
    }
}
