use crate::{DeployedUrl, Target};
use arkon_adapters::fingerprint::FileDiff;
use arkon_core::{
    artifact::Artifact,
    config::{S3Target as S3Config, TargetConfig},
    deploy::DeployCtx,
    error::{ArkonError, Result},
    runtime::CostHint,
};
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    config::{Builder as S3ConfigBuilder, Credentials, Region},
    primitives::ByteStream,
    Client as S3Client,
};
use std::{collections::HashMap, path::PathBuf};
use tracing::{debug, info, warn};

pub struct S3Target;

impl Target for S3Target {
    fn name(&self) -> &str { "s3" }

    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        // Block-in-place: the Target trait is sync but AWS SDK is async.
        // We create a single-threaded runtime here — the outer tokio runtime is on
        // the CLI layer. For sprint 3 we'll make Target async end-to-end.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ArkonError::Other(e.into()))?;

        rt.block_on(self.deploy_async(artifact, ctx, config))
    }

    fn cost_estimate(&self, artifact: &Artifact, config: &TargetConfig) -> Option<CostHint> {
        let (_, endpoint, _, _, _) = extract_s3_params(config).ok()?;
        let size_gb = artifact.size_bytes as f64 / 1_073_741_824.0;

        let (store_per_gb, egress_per_gb) = endpoint.as_deref()
            .map(|ep| {
                if ep.contains("backblazeb2") { (0.006_f64, 0.010_f64) }
                else if ep.contains("r2.cloudflarestorage") { (0.015, 0.0) }
                else { (0.023, 0.090) } // AWS S3
            })
            .unwrap_or((0.023, 0.090));

        let upload_usd = size_gb * store_per_gb;
        let egress_monthly_usd = size_gb * 10.0 * egress_per_gb;

        Some(CostHint {
            upload_usd,
            egress_monthly_usd,
            breakdown: format!(
                "{:.3}GB upload @ ${:.3}/GB = ${:.4}; est. egress ${:.4}/mo",
                size_gb, store_per_gb, upload_usd, egress_monthly_usd
            ),
        })
    }
}

impl S3Target {
    async fn deploy_async(
        &self,
        artifact: &Artifact,
        ctx: &DeployCtx,
        config: &TargetConfig,
    ) -> Result<DeployedUrl> {
        let (bucket, endpoint, region, invalidate, dist_id) = extract_s3_params(config)?;
        let client = build_client(&endpoint, &region, ctx).await?;

        info!(
            bucket = %bucket,
            files  = %artifact.file_hashes.len(),
            mb     = %(artifact.size_bytes / 1_048_576),
            "s3 diff-aware upload starting"
        );

        // 1. Fetch remote ETags (ListObjectsV2) for diff
        let remote_hashes = self.list_remote(&client, &bucket).await?;
        let diff = FileDiff::compute(&remote_hashes, &artifact.file_hashes);

        if diff.is_empty() && !artifact.meta.contains_key("rollback") {
            info!("artifact unchanged — skipping upload");
            return Ok(DeployedUrl::http(bucket_url(&bucket, &endpoint, &region)));
        }

        info!(
            added    = %diff.added.len(),
            modified = %diff.modified.len(),
            deleted  = %diff.deleted.len(),
            "diff computed"
        );

        // 2. Upload changed + added files
        for rel_path in diff.added.iter().chain(diff.modified.iter()) {
            let local_path = artifact.path.join(rel_path);
            if !local_path.exists() {
                warn!(key = %rel_path, "local file missing — skipping");
                continue;
            }
            self.put_object(
                &client, &bucket, rel_path, &local_path,
                mime_for(rel_path), cache_control_for(rel_path),
            ).await?;
        }

        // 3. Delete removed files
        if !diff.deleted.is_empty() {
            self.delete_objects(&client, &bucket, &diff.deleted).await?;
        }

        // 4. CDN invalidation
        if invalidate {
            match dist_id {
                Some(ref dist) => self.invalidate_cloudfront(&client, dist).await?,
                None           => self.invalidate_cloudflare(&bucket, ctx).await?,
            }
        }

        Ok(DeployedUrl::http(bucket_url(&bucket, &endpoint, &region)))
    }

    /// ListObjectsV2 → returns map of key → ETag (stripped of quotes).
    async fn list_remote(
        &self,
        client: &S3Client,
        bucket: &str,
    ) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        let mut continuation: Option<String> = None;

        loop {
            let mut req = client.list_objects_v2().bucket(bucket);
            if let Some(tok) = continuation {
                req = req.continuation_token(tok);
            }

            let resp = req.send().await
                .map_err(|e| ArkonError::Network(e.to_string()))?;

            for obj in resp.contents() {
                if let (Some(key), Some(etag)) = (obj.key(), obj.e_tag()) {
                    // AWS ETags are quoted; strip them and use as hash proxy
                    let clean_etag = etag.trim_matches('"').to_string();
                    map.insert(key.to_string(), clean_etag);
                }
            }

            match resp.next_continuation_token() {
                Some(tok) => continuation = Some(tok.to_string()),
                None => break,
            }
        }

        debug!(objects = %map.len(), bucket = %bucket, "listed remote objects");
        Ok(map)
    }

    async fn put_object(
        &self,
        client: &S3Client,
        bucket: &str,
        key: &str,
        path: &PathBuf,
        content_type: &str,
        cache_control: &str,
    ) -> Result<()> {
        let bytes = tokio::fs::read(path).await?;
        let len = bytes.len() as i64;
        let body = ByteStream::from(bytes);

        client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body)
            .content_type(content_type)
            .cache_control(cache_control)
            .content_length(len)
            .send()
            .await
            .map_err(|e| ArkonError::Network(format!("PUT {key}: {e}")))?;

        debug!(key = %key, bytes = %len, "uploaded");
        Ok(())
    }

    async fn delete_objects(
        &self,
        client: &S3Client,
        bucket: &str,
        keys: &[String],
    ) -> Result<()> {
        use aws_sdk_s3::types::{Delete, ObjectIdentifier};

        // S3 batch delete supports up to 1000 objects per request
        for chunk in keys.chunks(1000) {
            let objects: Vec<ObjectIdentifier> = chunk
                .iter()
                .filter_map(|k| ObjectIdentifier::builder().key(k).build().ok())
                .collect();

            let delete = Delete::builder()
                .set_objects(Some(objects))
                .build()
                .map_err(|e| ArkonError::Network(e.to_string()))?;

            client
                .delete_objects()
                .bucket(bucket)
                .delete(delete)
                .send()
                .await
                .map_err(|e| ArkonError::Network(format!("DELETE batch: {e}")))?;

            info!(count = %chunk.len(), "deleted stale objects");
        }
        Ok(())
    }

    async fn invalidate_cloudfront(&self, _client: &S3Client, distribution_id: &str) -> Result<()> {
        use aws_sigv4::{
            http_request::{sign, SignableBody, SignableRequest, SigningSettings},
            sign::v4,
        };
        use aws_credential_types::Credentials;

        let caller_ref = chrono::Utc::now().timestamp_millis().to_string();
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <InvalidationBatch>\
               <CallerReference>{caller_ref}</CallerReference>\
               <Paths><Quantity>1</Quantity><Items><Path>/*</Path></Items></Paths>\
             </InvalidationBatch>"
        );

        let url = format!(
            "https://cloudfront.amazonaws.com/2020-05-31/distribution/{distribution_id}/invalidation"
        );

        // Build the signing identity from environment / vault
        let key_id  = std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default();
        let secret  = std::env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_default();
        let token   = std::env::var("AWS_SESSION_TOKEN").ok();

        let creds = Credentials::new(&key_id, &secret, token, None, "arkon-sigv4");

        let identity = creds.into();
        let region   = aws_sdk_s3::config::Region::new("us-east-1");

        let mut signing_settings = SigningSettings::default();
        let signing_params = v4::SigningParams::builder()
            .identity(&identity)
            .region(region.as_ref())
            .name("cloudfront")
            .time(std::time::SystemTime::now())
            .settings(signing_settings)
            .build()
            .map_err(|e| ArkonError::Network(format!("SigV4 params: {e}")))?
            .into();

        let signable = SignableRequest::new(
            "POST",
            &url,
            std::iter::once(("content-type", "application/xml")),
            SignableBody::Bytes(body.as_bytes()),
        ).map_err(|e| ArkonError::Network(format!("signable request: {e}")))?;

        let (signing_instructions, _) = sign(signable, &signing_params)
            .map_err(|e| ArkonError::Network(format!("SigV4 sign: {e}")))?
            .into_parts();

        // Build the final signed HTTP request via reqwest
        let mut req = reqwest::Client::new()
            .post(&url)
            .header("Content-Type", "application/xml")
            .body(body);

        for (k, v) in signing_instructions.headers() {
            req = req.header(k, v);
        }

        let resp = req.send().await
            .map_err(|e| ArkonError::Network(format!("CloudFront request: {e}")))?;

        if resp.status().is_success() {
            info!(dist = %distribution_id, "CloudFront invalidation created (SigV4-signed)");
        } else {
            let status = resp.status();
            let text   = resp.text().await.unwrap_or_default();
            warn!(dist = %distribution_id, status = %status, body = %text,
                  "CloudFront invalidation non-2xx (non-fatal)");
        }
        Ok(())
    }

    async fn invalidate_cloudflare(&self, bucket: &str, ctx: &DeployCtx) -> Result<()> {
        // Cloudflare Cache Purge API — requires CF_API_TOKEN + ZONE_ID in vault
        if let (Some(token), Some(zone)) = (
            ctx.env.get("CF_API_TOKEN"),
            ctx.env.get("CF_ZONE_ID"),
        ) {
            let client = reqwest::Client::new();
            let url = format!("https://api.cloudflare.com/client/v4/zones/{zone}/purge_cache");
            let body = serde_json::json!({ "purge_everything": true });
            let resp = client
                .post(&url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .map_err(|e| ArkonError::Network(e.to_string()))?;

            if resp.status().is_success() {
                info!(bucket = %bucket, "Cloudflare cache purged");
            } else {
                warn!(status = %resp.status(), "Cloudflare purge returned non-200");
            }
        } else {
            debug!("CF_API_TOKEN / CF_ZONE_ID not in vault — skipping Cloudflare purge");
        }
        Ok(())
    }
}

fn extract_s3_params(config: &TargetConfig) -> Result<(String, Option<String>, String, bool, Option<String>)> {
    match config {
        TargetConfig::S3(c) => Ok((
            c.bucket.clone(),
            c.endpoint.clone(),
            c.region.clone().unwrap_or_else(|| "us-east-1".into()),
            c.invalidate,
            c.cloudfront_distribution_id.clone(),
        )),
        TargetConfig::B2(c) => Ok((
            c.bucket.clone(),
            Some("https://s3.us-west-004.backblazeb2.com".into()),
            "us-west-004".into(),
            false,
            None,
        )),
        TargetConfig::R2(c) => Ok((
            c.bucket.clone(),
            Some(format!("https://{}.r2.cloudflarestorage.com", c.account_id)),
            "auto".into(),
            c.invalidate,
            None,
        )),
        _ => Err(ArkonError::ConfigError("expected s3/b2/r2 target config".into())),
    }
}

fn bucket_url(bucket: &str, endpoint: &Option<String>, region: &str) -> String {
    if let Some(ep) = endpoint {
        format!("{}/{}", ep.trim_end_matches('/'), bucket)
    } else {
        format!("https://{}.s3.{}.amazonaws.com", bucket, region)
    }
}

/// Map file extension to Content-Type for correct browser rendering.
fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css"          => "text/css",
        "js" | "mjs"   => "application/javascript",
        "json"         => "application/json",
        "svg"          => "image/svg+xml",
        "png"          => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif"          => "image/gif",
        "webp"         => "image/webp",
        "ico"          => "image/x-icon",
        "wasm"         => "application/wasm",
        "txt"          => "text/plain",
        "xml"          => "application/xml",
        "pdf"          => "application/pdf",
        "woff"         => "font/woff",
        "woff2"        => "font/woff2",
        _              => "application/octet-stream",
    }
}

/// Appropriate Cache-Control header per file type.
fn cache_control_for(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    // Hashed assets contain a content hash in their filename segment.
    // Patterns: main.a1b2c3d4.js  chunk.deadbeef12345678.css  index-Ab3Cd9Ef.js
    // We detect a hex run of ≥8 chars separated by '.' or '-' in the filename stem.
    let filename = path.rsplit('/').next().unwrap_or(path);
    let looks_hashed = {
        let mut chars = filename.chars().peekable();
        let mut run   = 0usize;
        let mut found = false;
        while let Some(c) = chars.next() {
            if c.is_ascii_hexdigit() {
                run += 1;
                if run >= 8 { found = true; break; }
            } else if matches!(c, '.' | '-' | '_') {
                run = 0;
            } else {
                run = 0;
            }
        }
        found
    } || path.contains(".chunk.");

    if looks_hashed {
        return "public, max-age=31536000, immutable";
    }
    match ext {
        "html" | "htm"        => "public, max-age=0, must-revalidate",
        "css" | "js" | "mjs"  => "public, max-age=86400",
        "wasm"                => "public, max-age=604800",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico"
                              => "public, max-age=2592000",
        "woff" | "woff2"      => "public, max-age=31536000, immutable",
        _                     => "public, max-age=3600",
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Build an authenticated S3 client. Credentials sourced from:
/// 1. DeployCtx env (from ARKON vault: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
/// 2. Standard AWS env vars / ~/.aws/credentials chain (aws_config default)
async fn build_client(
    endpoint: &Option<String>,
    region: &str,
    ctx: &DeployCtx,
) -> Result<S3Client> {
    let mut cfg_builder = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(region.to_string()));

    // Inject credentials from vault if present
    if let (Some(key_id), Some(secret)) = (
        ctx.env.get("AWS_ACCESS_KEY_ID"),
        ctx.env.get("AWS_SECRET_ACCESS_KEY"),
    ) {
        let token = ctx.env.get("AWS_SESSION_TOKEN").cloned();
        let creds = Credentials::new(key_id, secret, token, None, "arkon-vault");
        cfg_builder = cfg_builder.credentials_provider(creds);
    }

    let aws_cfg = cfg_builder.load().await;
    let mut s3_builder = S3ConfigBuilder::from(&aws_cfg);

    // Custom endpoint for B2, R2, MinIO, etc.
    if let Some(ep) = endpoint {
        s3_builder = s3_builder
            .endpoint_url(ep)
            .force_path_style(true);
    }

    Ok(S3Client::from_conf(s3_builder.build()))
}
