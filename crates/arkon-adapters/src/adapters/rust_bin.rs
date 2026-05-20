use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};
use std::path::PathBuf;

pub struct RustBinAdapter;

impl Adapter for RustBinAdapter {
    fn name(&self) -> &str { "rust-bin" }
    fn description(&self) -> &str { "Rust binary — Axum, Actix, or any Cargo project" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        let cmd = ctx.command_override.as_deref()
            .unwrap_or("cargo build --release");
        runner::run_shell_command(cmd, &ctx.root, &ctx.env)?;

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("rust-bin", output, DeployableKind::Binary);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;

        // Detect binary name from Cargo.toml [[bin]] or package name
        let bin_name = detect_bin_name(&ctx.root);
        art.meta.insert("binary".into(), bin_name.clone());
        art.meta.insert("start_command".into(), format!("./{}", bin_name));
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        ctx.root.join("target/release")
    }

    fn runtime_info(&self) -> Runtime { Runtime::native() }
    fn deployable_type(&self) -> DeployableKind { DeployableKind::Binary }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(&ctx.root, &["Cargo.toml", "Cargo.lock"])
    }
}

fn detect_bin_name(root: &PathBuf) -> String {
    let cargo_toml = root.join("Cargo.toml");
    if let Ok(raw) = std::fs::read_to_string(&cargo_toml) {
        if let Ok(v) = raw.parse::<toml::Value>() {
            if let Some(name) = v.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
                return name.replace('-', "_");
            }
        }
    }
    "app".into()
}
