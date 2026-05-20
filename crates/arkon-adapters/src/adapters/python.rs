use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::{Runtime, RuntimeKind}};
use std::path::PathBuf;

pub struct PythonAdapter;

impl Adapter for PythonAdapter {
    fn name(&self) -> &str { "python" }
    fn description(&self) -> &str { "Python application — Flask, FastAPI, Django" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        // Create a virtualenv in .arkon-venv and install deps
        runner::run_shell_command(
            "python3 -m venv .arkon-venv && .arkon-venv/bin/pip install -r requirements.txt -q",
            &ctx.root,
            &ctx.env,
        )?;

        // If Django: run collectstatic
        if ctx.root.join("manage.py").exists() {
            runner::run_shell_command(
                ".arkon-venv/bin/python manage.py collectstatic --noinput",
                &ctx.root,
                &ctx.env,
            ).ok(); // non-fatal
        }

        let output = self.output_dir(ctx);
        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("python", output, DeployableKind::Binary);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes = size;
        art.meta.insert("start_command".into(), detect_start_cmd(&ctx.root));
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        // For Python we deploy the whole project root (with venv embedded)
        ctx.root.clone()
    }

    fn runtime_info(&self) -> Runtime {
        Runtime {
            kind: RuntimeKind::Python,
            version: Some("3.11".into()),
            env_keys: vec!["DATABASE_URL".into(), "SECRET_KEY".into(), "PORT".into()],
        }
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Binary }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(
            &ctx.root,
            &["requirements.txt", "pyproject.toml", "setup.py", "setup.cfg"],
        )
    }
}

fn detect_start_cmd(root: &PathBuf) -> String {
    if root.join("manage.py").exists() {
        return "gunicorn wsgi:application".into();
    }
    if root.join("main.py").exists() {
        return "uvicorn main:app --host 0.0.0.0 --port $PORT".into();
    }
    if root.join("app.py").exists() {
        return "gunicorn app:app".into();
    }
    "python main.py".into()
}
