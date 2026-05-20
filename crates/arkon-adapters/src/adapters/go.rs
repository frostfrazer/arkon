use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::{Runtime, RuntimeKind}};
use std::path::PathBuf;

pub struct GoAdapter;

impl Adapter for GoAdapter {
    fn name(&self) -> &str { "go" }
    fn description(&self) -> &str { "Go application — compiles to a single static binary" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        let project_name = ctx.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app");
        let out_bin = ctx.root.join("dist").join(project_name);
        std::fs::create_dir_all(ctx.root.join("dist"))?;

        let cmd = ctx.command_override.clone().unwrap_or_else(|| {
            format!(
                "CGO_ENABLED=0 GOOS=linux go build -ldflags='-s -w' -o dist/{} ./...",
                project_name
            )
        });
        runner::run_shell_command(&cmd, &ctx.root, &ctx.env)?;

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("go", output, DeployableKind::Binary);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        art.meta.insert("binary".into(), project_name.into());
        art.meta.insert("start_command".into(), format!("./{}", project_name));
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        ctx.root.join("dist")
    }

    fn runtime_info(&self) -> Runtime {
        Runtime {
            kind: RuntimeKind::Native,
            version: None,
            env_keys: vec!["PORT".into(), "DATABASE_URL".into()],
        }
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Binary }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(&ctx.root, &["go.mod", "go.sum"])
    }
}
