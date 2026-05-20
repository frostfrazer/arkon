use crate::{DeployedUrl, DeployedUrlKind, Target};
use arkon_core::{
    artifact::{Artifact, DeployableKind},
    config::TargetConfig,
    deploy::DeployCtx,
    error::{ArkonError, Result},
};
use bollard::{
    Docker,
    image::{BuildImageOptions, PushImageOptions},
    auth::DockerCredentials,
};
use futures_util::stream::StreamExt;
use std::io::Read;
use tracing::{debug, info, warn};

pub struct DockerTarget;

impl Target for DockerTarget {
    fn name(&self) -> &str { "docker" }

    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ArkonError::Other(e.into()))?;
        rt.block_on(self.deploy_async(artifact, ctx, config))
    }
}

impl DockerTarget {
    async fn deploy_async(
        &self,
        artifact: &Artifact,
        ctx: &DeployCtx,
        _config: &TargetConfig,
    ) -> Result<DeployedUrl> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("Docker not available: {e}. Is Docker running?")))?;

        let image_name = format!("arkon/{}", sanitize(&ctx.project_name));
        let tag        = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
        let image_ref  = format!("{image_name}:{tag}");

        // 1. Ensure a Dockerfile exists (generate one if not present)
        let dockerfile_path = artifact.path.join("Dockerfile");
        if !dockerfile_path.exists() {
            let generated = self.generate_dockerfile(artifact)?;
            std::fs::write(&dockerfile_path, generated)?;
            info!("generated Dockerfile for {} artifact", format!("{:?}", artifact.kind));
        }

        // 2. Create tar context for Docker build
        info!(image = %image_ref, "building Docker image");
        let tar_bytes = self.create_build_context(&artifact.path)?;

        // 3. Build image
        let options = BuildImageOptions {
            t: image_ref.clone(),
            rm: true,
            forcerm: true,
            ..Default::default()
        };

        let mut build_stream = docker.build_image(
            options,
            None,
            Some(tar_bytes.into()),
        );

        while let Some(msg) = build_stream.next().await {
            match msg {
                Ok(info) => {
                    if let Some(stream) = info.stream {
                        let trimmed = stream.trim();
                        if !trimmed.is_empty() {
                            debug!(build = %trimmed);
                        }
                    }
                    if let Some(err) = info.error {
                        return Err(ArkonError::BuildFailed(format!("Docker build: {err}")));
                    }
                }
                Err(e) => return Err(ArkonError::BuildFailed(e.to_string())),
            }
        }

        info!(image = %image_ref, "Docker image built");

        // 4. If a registry is configured, push
        if let Some(registry) = ctx.env.get("DOCKER_REGISTRY") {
            let remote_ref = format!("{registry}/{image_ref}");
            docker.tag_image(&image_ref, Some(bollard::image::TagImageOptions {
                repo: format!("{registry}/{image_name}"),
                tag:  tag.clone(),
            })).await.map_err(|e| ArkonError::Network(e.to_string()))?;

            let creds = DockerCredentials {
                username: ctx.env.get("DOCKER_USERNAME").cloned(),
                password: ctx.env.get("DOCKER_PASSWORD").cloned(),
                ..Default::default()
            };

            info!(image = %remote_ref, "pushing to registry");
            let mut push_stream = docker.push_image(
                &format!("{registry}/{image_name}"),
                Some(PushImageOptions { tag: tag.clone() }),
                Some(creds),
            );

            while let Some(msg) = push_stream.next().await {
                if let Err(e) = msg {
                    return Err(ArkonError::Network(format!("Docker push: {e}")));
                }
            }

            info!(image = %remote_ref, "image pushed to registry");
            return Ok(DeployedUrl { url: remote_ref, kind: DeployedUrlKind::Http });
        }

        // 5. No registry — image is local. Caller (SSH target) can docker save + scp.
        info!(image = %image_ref, "image built locally (no registry configured)");
        Ok(DeployedUrl { url: format!("docker://{image_ref}"), kind: DeployedUrlKind::Local })
    }

    fn create_build_context(&self, root: &std::path::Path) -> Result<bytes::Bytes> {
        use std::io::Write;

        let tmp = tempfile::NamedTempFile::new()?;
        let path = tmp.path().to_path_buf();

        {
            let file = std::fs::File::create(&path)?;
            let mut builder = tar::Builder::new(file);
            builder.append_dir_all(".", root)
                .map_err(|e| ArkonError::BuildFailed(format!("tar context: {e}")))?;
            builder.finish()?;
        }

        let bytes = std::fs::read(&path)?;
        Ok(bytes::Bytes::from(bytes))
    }

    fn generate_dockerfile(&self, artifact: &Artifact) -> Result<String> {
        let dockerfile = match artifact.kind {
            DeployableKind::Static => r#"FROM nginx:alpine
COPY . /usr/share/nginx/html
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
"#.to_string(),

            DeployableKind::Container => {
                // Node.js server
                r#"FROM node:20-alpine
WORKDIR /app
COPY package*.json ./
RUN npm ci --only=production
COPY . .
EXPOSE 3000
CMD ["node", "server.js"]
"#.to_string()
            }

            DeployableKind::Binary => {
                r#"FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN chmod +x ./app
EXPOSE 8080
CMD ["./app"]
"#.to_string()
            }

            _ => r#"FROM debian:bookworm-slim
WORKDIR /app
COPY . .
EXPOSE 8080
CMD ["./entrypoint.sh"]
"#.to_string(),
        };
        Ok(dockerfile)
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .to_lowercase()
}
