//! Minimal async HTTP static file server.
//! Serves the build artifact directory over localhost so the relay can proxy it.

use arkon_core::error::{ArkonError, Result};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::{debug, info, warn};

/// A running local HTTP file server.
pub struct LocalServer {
    pub addr: SocketAddr,
    pub root: PathBuf,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl LocalServer {
    /// Start the server. Port 0 = OS picks a free port.
    pub async fn start(root: &Path, port: u16) -> Result<Self> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .map_err(|e| ArkonError::Other(e.into()))?;

        let addr = listener.local_addr()
            .map_err(|e| ArkonError::Other(e.into()))?;

        let root_arc = Arc::new(root.to_path_buf());
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Ok((stream, peer)) = listener.accept() => {
                        let root = root_arc.clone();
                        debug!(peer = %peer, "connection accepted");
                        tokio::spawn(handle_connection(stream, root));
                    }
                    _ = &mut rx => {
                        info!("local server shutting down");
                        break;
                    }
                }
            }
        });

        info!(addr = %addr, root = %root.display(), "local file server started");
        Ok(Self { addr, root: root.to_path_buf(), shutdown_tx: tx })
    }

    /// Gracefully stop the server.
    pub fn stop(self) {
        let _ = self.shutdown_tx.send(());
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

async fn handle_connection(mut stream: TcpStream, root: Arc<PathBuf>) {
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buf[..n]);
    let path = parse_request_path(&request);
    let file_path = resolve_path(&root, &path);

    let (status, content_type, body, etag, last_modified) = match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let ct = mime_for(&file_path.to_string_lossy());
            // Compute ETag as hex-encoded SHA-256 of content
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&bytes);
            let etag = format!("\"{}\"", &hex::encode(h.finalize())[..16]);
            // Last-Modified from file metadata
            let lm = tokio::fs::metadata(&file_path).await
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| httpdate::fmt_http_date(t))
                .unwrap_or_else(|| "Thu, 01 Jan 1970 00:00:00 GMT".to_string());
            (200u16, ct, bytes, etag, lm)
        }
        Err(_) => {
            match tokio::fs::read(root.join("index.html")).await {
                Ok(bytes) => {
                    use sha2::{Digest, Sha256};
                    let mut h = Sha256::new();
                    h.update(&bytes);
                    let etag = format!("\"{}\"", &hex::encode(h.finalize())[..16]);
                    (200, "text/html; charset=utf-8", bytes, etag,
                     "Thu, 01 Jan 1970 00:00:00 GMT".to_string())
                }
                Err(_) => (404, "text/plain", b"404 Not Found".to_vec(),
                           "\"404\"".to_string(), "Thu, 01 Jan 1970 00:00:00 GMT".to_string()),
            }
        }
    };

    // Check If-None-Match for conditional GET support
    let request_str = String::from_utf8_lossy(&buf[..n]);
    let client_etag = request_str.lines()
        .find(|l| l.to_lowercase().starts_with("if-none-match:"))
        .and_then(|l| l.splitn(2, ':').nth(1))
        .map(|v| v.trim().to_string());

    if let Some(ref client_e) = client_etag {
        if client_e == &etag && status == 200 {
            let header = format!(
                "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nX-Served-By: ARKON-Preview\r\n\r\n"
            );
            let _ = stream.write_all(header.as_bytes()).await;
            return;
        }
    }

    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nETag: {etag}\r\nLast-Modified: {last_modified}\r\n\
         X-Served-By: ARKON-Preview\r\n\r\n",
        if status == 200 { "OK" } else if status == 304 { "Not Modified" } else { "Not Found" },
        body.len()
    );

    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(&body).await;
}

fn parse_request_path(request: &str) -> String {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string()
}

fn resolve_path(root: &Path, url_path: &str) -> PathBuf {
    let rel = url_path.trim_start_matches('/');
    let candidate = root.join(rel);
    if candidate.is_dir() {
        candidate.join("index.html")
    } else {
        candidate
    }
}

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
        "wasm"         => "application/wasm",
        "ico"          => "image/x-icon",
        "woff2"        => "font/woff2",
        _              => "application/octet-stream",
    }
}
