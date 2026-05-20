use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};
use std::path::PathBuf;

pub struct NodejsAdapter;

impl Adapter for NodejsAdapter {
    fn name(&self) -> &str { "nodejs" }
    fn description(&self) -> &str { "Node.js server — Express, Fastify, Hono, etc." }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        // Only run a build step if one is configured or a build script exists
        let has_build = ctx.command_override.is_some()
            || has_npm_script(&ctx.root, "build");

        if has_build {
            let cmd = ctx.command_override.as_deref().unwrap_or("npm run build");
            runner::run_shell_command(cmd, &ctx.root, &ctx.env)?;
        }

        // Install prod deps into output
        runner::run_shell_command(
            "npm ci --omit=dev --prefer-offline",
            &ctx.root,
            &ctx.env,
        )?;

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("nodejs", output, DeployableKind::Binary);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        art.meta.insert("start_command".into(), detect_start_cmd(&ctx.root));
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        // Prefer dist/ if present (TypeScript projects), else the root
        let dist = ctx.root.join("dist");
        if dist.exists() { dist } else { ctx.root.clone() }
    }

    fn runtime_info(&self) -> Runtime {
        Runtime::node("20").with_env(["NODE_ENV", "PORT", "DATABASE_URL"])
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Binary }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(
            &ctx.root,
            &["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "tsconfig.json"],
        )
    }
}

fn has_npm_script(root: &PathBuf, script: &str) -> bool {
    let pkg = root.join("package.json");
    if !pkg.exists() { return false; }
    std::fs::read_to_string(&pkg)
        .map(|s| s.contains(&format!("\"{}\"", script)))
        .unwrap_or(false)
}

fn detect_start_cmd(root: &PathBuf) -> String {
    // Try to read "start" script from package.json, else fallback
    let pkg = root.join("package.json");
    if let Ok(raw) = std::fs::read_to_string(&pkg) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(start) = v["scripts"]["start"].as_str() {
                return start.to_string();
            }
        }
    }
    "node index.js".into()
}
