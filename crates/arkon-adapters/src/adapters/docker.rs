use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::{Runtime, RuntimeKind}};
use std::path::PathBuf;

pub struct DockerAdapter;

impl Adapter for DockerAdapter {
    fn name(&self) -> &str { "docker" }
    fn description(&self) -> &str { "Docker container — builds image from local Dockerfile" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        let project_name = ctx.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("arkon-app");
        let tag = format!("{}:arkon-latest", project_name);

        let cmd = ctx.command_override.clone()
            .unwrap_or_else(|| format!("docker build -t {} .", tag));
        runner::run_shell_command(&cmd, &ctx.root, &ctx.env)?;

        // Save the image to a tarball in dist/
        let dist = ctx.root.join("dist");
        std::fs::create_dir_all(&dist)?;
        let tarball = dist.join(format!("{}.tar", project_name));
        runner::run_shell_command(
            &format!("docker save {} -o {}", tag, tarball.display()),
            &ctx.root,
            &ctx.env,
        )?;

        let (fp, hashes, size) = fingerprint::fingerprint_dir(&dist)?;
        let mut art = Artifact::new("docker", dist, DeployableKind::Container);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        art.meta.insert("image_tag".into(), tag);
        art.meta.insert("tarball".into(), tarball.to_string_lossy().into());
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        ctx.root.join("dist")
    }

    fn runtime_info(&self) -> Runtime {
        Runtime {
            kind: RuntimeKind::Docker,
            version: None,
            env_keys: vec![],
        }
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Container }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(&ctx.root, &["Dockerfile", ".dockerignore"])
    }
}
