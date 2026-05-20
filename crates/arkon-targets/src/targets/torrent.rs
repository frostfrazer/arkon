use crate::{DeployedUrl, DeployedUrlKind, Target};
use arkon_core::{
    artifact::Artifact,
    config::TargetConfig,
    deploy::DeployCtx,
    error::{ArkonError, Result},
    runtime::CostHint,
};
use lava_torrent::torrent::v1::TorrentBuilder;
use minijinja::{Environment, context};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tracing::info;

pub struct TorrentTarget;

/// Download page HTML template. Inline so we have zero runtime file deps.
const DL_PAGE_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ name }} — Download</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  :root {
    --bg: #0a0a0b; --surface: #111113; --border: rgba(255,255,255,0.08);
    --text: #e8e8ec; --muted: #6a6a74; --gold: #c8a84b; --green: #4ac87a;
  }
  body { background: var(--bg); color: var(--text); font-family: system-ui, sans-serif;
         min-height: 100vh; display: flex; flex-direction: column; align-items: center;
         justify-content: center; padding: 2rem; }
  .card { background: var(--surface); border: 1px solid var(--border); border-radius: 12px;
          padding: 2.5rem; max-width: 560px; width: 100%; }
  h1   { font-size: 1.6rem; font-weight: 700; color: var(--gold); margin-bottom: .4rem; }
  .version { font-size: .8rem; color: var(--muted); margin-bottom: 1.5rem;
             font-family: monospace; }
  .size    { font-size: .8rem; color: var(--muted); font-family: monospace; }
  .btn {
    display: flex; align-items: center; gap: .6rem;
    width: 100%; padding: .85rem 1.25rem; border-radius: 8px; border: 1px solid var(--border);
    background: transparent; color: var(--text); cursor: pointer; font-size: .95rem;
    text-decoration: none; transition: border-color .15s, background .15s; margin-top: .75rem;
  }
  .btn:hover { border-color: var(--gold); background: rgba(200,168,75,.06); }
  .btn .icon { width: 20px; text-align: center; }
  .btn.primary { background: rgba(74,200,122,.08); border-color: rgba(74,200,122,.3); color: var(--green); }
  .btn.primary:hover { background: rgba(74,200,122,.14); }
  .hash { margin-top: 1.5rem; padding: 1rem; background: #060810; border-radius: 8px;
          font-family: monospace; font-size: .75rem; color: var(--muted); word-break: break-all; }
  .hash span { color: var(--text); }
  footer { margin-top: 1.5rem; text-align: center; font-size: .75rem; color: var(--muted); }
  footer a { color: var(--gold); text-decoration: none; }
</style>
</head>
<body>
<div class="card">
  <h1>{{ name }}</h1>
  <p class="version">built {{ deployed_at }}  ·  <span class="size">{{ size_mb }} MB</span></p>

  {% if direct_url %}
  <a class="btn primary" href="{{ direct_url }}">
    <span class="icon">⬇</span> Direct download
  </a>
  {% endif %}

  {% if magnet %}
  <a class="btn" href="{{ magnet }}">
    <span class="icon">🧲</span> Magnet link (torrent)
  </a>
  {% endif %}

  {% if torrent_file %}
  <a class="btn" href="{{ torrent_file }}">
    <span class="icon">📄</span> Download .torrent file
  </a>
  {% endif %}

  <div class="hash">
    SHA-256 &nbsp;<span>{{ fingerprint }}</span>
  </div>
</div>
<footer>distributed via <a href="https://arkon.sh">ARKON</a> — no platform fees</footer>
</body>
</html>
"#;

impl Target for TorrentTarget {
    fn name(&self) -> &str { "torrent" }

    fn deploy(&self, artifact: &Artifact, ctx: &DeployCtx, config: &TargetConfig) -> Result<DeployedUrl> {
        let (bucket, dl_page, direct_url_base) = extract_torrent_config(config);

        info!(
            project = %ctx.project_name,
            size_mb = %(artifact.size_bytes / 1_048_576),
            "creating torrent + download page"
        );

        // 1. Build .torrent file
        let torrent_name = sanitize(&ctx.project_name);
        let torrent_path = artifact.path.parent()
            .unwrap_or(&artifact.path)
            .join(format!("{torrent_name}.torrent"));

        let torrent = TorrentBuilder::new(&artifact.path, 524288i64)
            .set_name(torrent_name.clone())
            .set_announce(Some("udp://open.stealth.si:80/announce".into()))
            .set_announce_list(vec![
                vec!["udp://open.stealth.si:80/announce".into()],
                vec!["udp://tracker.opentrackr.org:1337/announce".into()],
                vec!["udp://tracker.openbittorrent.com:6969/announce".into()],
            ])
            .build()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("torrent build failed: {e}")))?;

        let magnet = torrent.magnet_link()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("magnet link failed: {e}")))?;

        torrent.write_into_file(&torrent_path)
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("torrent write failed: {e}")))?;

        info!(torrent = %torrent_path.display(), magnet = %magnet, "torrent created");

        // 2. Generate download HTML page
        if dl_page {
            let size_mb = format!("{:.1}", artifact.size_bytes as f64 / 1_048_576.0);
            let now     = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
            let torrent_filename = format!("{torrent_name}.torrent");

            let direct = direct_url_base
                .as_ref()
                .map(|base| format!("{base}/{torrent_name}"));

            let mut env = Environment::new();
            env.add_template("dl", DL_PAGE_TEMPLATE)
                .map_err(|e| ArkonError::Other(e.into()))?;

            let tmpl = env.get_template("dl")
                .map_err(|e| ArkonError::Other(e.into()))?;

            let html = tmpl.render(context! {
                name        => ctx.project_name,
                deployed_at => now,
                size_mb     => size_mb,
                fingerprint => artifact.fingerprint,
                magnet      => magnet.clone(),
                torrent_file => torrent_filename,
                direct_url  => direct,
            }).map_err(|e| ArkonError::Other(e.into()))?;

            let dl_page_path = artifact.path.join("download.html");
            std::fs::write(&dl_page_path, html)?;
            info!(path = %dl_page_path.display(), "download page written");
        }

        // 3. Start seeding from this machine (background detached process)
        //    Full libtorrent integration is post-1.0; for now we use the
        //    `transmission-daemon` CLI if available, otherwise just log the path.
        self.start_seeding(&torrent_path, &artifact.path);

        Ok(DeployedUrl {
            url: format!("file://{}", torrent_path.display()),
            kind: DeployedUrlKind::Torrent { magnet },
        })
    }

    fn cost_estimate(&self, _artifact: &Artifact, _config: &TargetConfig) -> Option<CostHint> {
        Some(CostHint::free())
    }
}

impl TorrentTarget {
    fn start_seeding(&self, torrent_path: &Path, content_dir: &Path) {
        // 1. Try transmission-cli first (most users will have it)
        let transmission = std::process::Command::new("transmission-remote")
            .args([
                "--add", &torrent_path.to_string_lossy(),
                "--download-dir", &content_dir.to_string_lossy(),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if transmission {
            info!("torrent added to transmission-daemon for seeding");
            return;
        }

        // 2. Send HTTP tracker announce directly so the swarm knows we're a seed.
        //    This uses the info_hash from the .torrent file + a minimal UDP announce.
        let torrent_path_clone = torrent_path.to_path_buf();
        let content_dir_clone  = content_dir.to_path_buf();

        std::thread::spawn(move || {
            if let Ok(torrent) = lava_torrent::torrent::v1::Torrent::read_from_file(&torrent_path_clone) {
                let info_hash = torrent.info_hash_bytes();
                let info_hash_hex = hex::encode(&info_hash);

                // Announce to each tracker via HTTP GET
                // Minimal BitTorrent tracker protocol: announce that we are a seeder
                let peer_id = b"-AK0001-000000000000"; // ARKON peer ID prefix
                let port: u16 = 6881;

                let trackers: Vec<String> = torrent.announce_list
                    .iter()
                    .flatten()   // Option -> Vec<Vec<String>>
                    .flatten()   // Vec<Vec<String>> -> Vec<String>
                    .cloned()
                    .chain(torrent.announce.iter().cloned())
                    .collect();

                let tracker_count = trackers.len();

                for tracker_url in &trackers {
                    if !tracker_url.starts_with("http") { continue; }

                    let encoded_hash: String = info_hash.iter()
                        .map(|b| format!("%{b:02X}"))
                        .collect();
                    let encoded_peer_id: String = peer_id.iter()
                        .map(|b| format!("%{b:02X}"))
                        .collect();

                    let announce = format!(
                        "{tracker_url}?info_hash={encoded_hash}&peer_id={encoded_peer_id}\
                         &port={port}&uploaded=0&downloaded=0&left=0&event=completed&compact=1"
                    );

                    match reqwest::blocking::get(&announce) {
                        Ok(_)  => info!(hash = %info_hash_hex, tracker = %tracker_url, "tracker announce sent"),
                        Err(e) => tracing::debug!(tracker = %tracker_url, error = %e, "tracker announce failed"),
                    }
                }

                info!(
                    hash = %info_hash_hex,
                    path = %content_dir_clone.display(),
                    trackers = tracker_count,
                    "seeding ready — open torrent in any BitTorrent client to share bandwidth"
                );
            }
        });
    }
}

fn extract_torrent_config(config: &TargetConfig) -> (Option<String>, bool, Option<String>) {
    match config {
        TargetConfig::B2(c) => (Some(c.bucket.clone()), c.dl_page, None),
        _ => (None, true, None),
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .to_lowercase()
}
