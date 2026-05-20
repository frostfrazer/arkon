use crate::{Adapter, BuildCtx, fingerprint};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};
use std::path::PathBuf;

pub struct StaticAdapter;

impl Adapter for StaticAdapter {
    fn name(&self) -> &str { "static" }
    fn description(&self) -> &str { "Plain static HTML site — no build step" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        // No build command needed — the root IS the artifact
        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("static", output, DeployableKind::Static);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override {
            return ctx.root.join(d);
        }
        // If a dist/ or public/ folder exists, use that; otherwise use root
        for candidate in &["dist", "public", "out", "_site"] {
            let p = ctx.root.join(candidate);
            if p.exists() { return p; }
        }
        ctx.root.clone()
    }

    fn runtime_info(&self) -> Runtime { Runtime::static_files() }
    fn deployable_type(&self) -> DeployableKind { DeployableKind::Static }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(&ctx.root, &["index.html", "style.css", "script.js"])
    }
}
