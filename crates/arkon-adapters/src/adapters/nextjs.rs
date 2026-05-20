use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};
use std::path::PathBuf;

pub struct NextjsAdapter;

impl Adapter for NextjsAdapter {
    fn name(&self) -> &str { "nextjs" }
    fn description(&self) -> &str { "Next.js application (static export or standalone server)" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        let cmd = ctx.command_override.as_deref().unwrap_or("npm run build");
        runner::run_shell_command(cmd, &ctx.root, &ctx.env)?;

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("nextjs", output, DeployableKind::Static);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        // Next.js static export goes to `out/`; standalone server stays in `.next/`
        let out = ctx.root.join("out");
        if out.exists() { out } else { ctx.root.join(".next") }
    }

    fn runtime_info(&self) -> Runtime {
        Runtime::node("20").with_env(["NODE_ENV", "NEXT_PUBLIC_API_URL"])
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Static }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(
            &ctx.root,
            &["package-lock.json", "yarn.lock", "pnpm-lock.yaml",
              "next.config.js", "next.config.ts", "next.config.mjs"],
        )
    }
}
