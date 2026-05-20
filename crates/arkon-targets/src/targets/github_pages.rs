use crate::{DeployedUrl, DeployedUrlKind, Target};
use arkon_core::{
    artifact::Artifact,
    config::TargetConfig,
    deploy::DeployCtx,
    error::{ArkonError, Result},
};
use arkon_adapters::runner::run_shell_command;
use std::{collections::HashMap, path::PathBuf};
use tracing::{debug, info};

pub struct GithubPagesTarget;

impl Target for GithubPagesTarget {
    fn name(&self) -> &str { "github-pages" }

    fn health_check(&self, _config: &TargetConfig) -> Result<()> {
        // Verify git is available
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map_err(|_| ArkonError::Other(anyhow::anyhow!(
                "git not found in PATH — required for github-pages target"
            )))?;
        Ok(())
    }

    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let (repo, branch, cname) = extract_gh_config(config)?;

        info!(
            repo   = %repo,
            branch = %branch,
            files  = %artifact.file_hashes.len(),
            "deploying to GitHub Pages"
        );

        // We operate in a temp directory so we don't pollute the project working tree
        let tmp = tempfile::TempDir::new()
            .map_err(|e| ArkonError::Other(e.into()))?;
        let work = tmp.path();

        let env: HashMap<String, String> = ctx.env.clone();

        // 1. Clone just the target branch (shallow, no history we don't need)
        let clone_url = github_clone_url(&repo, &ctx.env);
        let clone_cmd = format!(
            "git clone --depth=1 --branch={branch} --single-branch {clone_url} .",
        );

        if run_shell_command(&clone_cmd, work, &env).is_err() {
            // Branch doesn't exist yet — init a fresh orphan branch
            info!(branch = %branch, "branch not found — initialising fresh gh-pages branch");
            run_shell_command("git init", work, &env)?;
            run_shell_command(&format!("git checkout --orphan {branch}"), work, &env)?;
            run_shell_command(&format!("git remote add origin {clone_url}"), work, &env)?;
        }

        // 2. Wipe everything except .git
        run_shell_command(
            "find . -mindepth 1 -not -path './.git/*' -not -name '.git' -delete",
            work, &env,
        )?;

        // 3. Copy artifact files into work dir
        copy_dir_all(&artifact.path, work)
            .map_err(|e| ArkonError::Other(e.into()))?;

        // 4. Write CNAME if configured
        if let Some(ref domain) = cname {
            std::fs::write(work.join("CNAME"), domain)?;
            info!(cname = %domain, "wrote CNAME");
        }

        // 5. Add .nojekyll so GitHub Pages serves files as-is
        std::fs::write(work.join(".nojekyll"), "")?;

        // 6. Commit and push
        run_shell_command("git add -A", work, &env)?;

        let commit_msg = format!(
            "arkon deploy {} — {}",
            &artifact.fingerprint[..12],
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        );
        let commit_cmd = format!("git commit -m '{commit_msg}' --allow-empty");
        run_shell_command(&commit_cmd, work, &env)?;

        run_shell_command(
            &format!("git push --force origin {branch}"),
            work, &env,
        )?;

        info!(branch = %branch, repo = %repo, "pushed to GitHub Pages");

        // Derive live URL
        let gh_url = cname
            .map(|d| format!("https://{d}"))
            .unwrap_or_else(|| {
                // e.g. "owner/repo" → https://owner.github.io/repo
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() >= 2 {
                    format!("https://{}.github.io/{}", parts[0], parts[1])
                } else {
                    format!("https://github.com/{repo}")
                }
            });

        Ok(DeployedUrl { url: gh_url, kind: DeployedUrlKind::Http })
    }
}

fn extract_gh_config(config: &TargetConfig) -> Result<(String, String, Option<String>)> {
    match config {
        TargetConfig::GithubPages(c) => Ok((
            c.repo.clone(),
            c.branch.clone().unwrap_or_else(|| "gh-pages".into()),
            c.cname.clone(),
        )),
        _ => Err(ArkonError::ConfigError("expected github-pages target config".into())),
    }
}

fn github_clone_url(repo: &str, env: &HashMap<String, String>) -> String {
    if let Some(token) = env.get("GITHUB_TOKEN") {
        format!("https://{token}@github.com/{repo}.git")
    } else {
        format!("https://github.com/{repo}.git")
    }
}

fn copy_dir_all(src: &PathBuf, dst: &std::path::Path) -> std::io::Result<()> {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
