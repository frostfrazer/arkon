//! ARKON GUI â€” Tauri IPC command handlers (full operational)
//!
//! Every command here is either:
//!   READ  â€” queries state without side effects (detect, list, status, history)
//!   WRITE â€” triggers real pipeline operations (ship, rollback, promote, secrets)
//!
//! Write commands accept a `tauri::ipc::Channel<ProgressEvent>` so the frontend
//! receives a real-time stream of log lines as the build/deploy runs.

use crate::state::AppState;
use arkon_adapters::{AdapterRegistry, BuildCtx, BuildRunner};
use arkon_core::{
    config::ArkonConfig,
    deploy::DeployCtx,
};
use arkon_detector::detect;
use arkon_secrets::Vault;
use arkon_store::SnapshotStore;
use arkon_targets::Dispatcher;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tauri::{ipc::Channel, State};

// â”€â”€â”€ Progress event (streamed to frontend) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    Log     { message: String },
    Step    { step: String, detail: String },
    Success { message: String, url: Option<String> },
    Error   { message: String },
    Done,
}

// â”€â”€â”€ Response types â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Serialize)]
pub struct DetectResult {
    pub adapter:     String,
    pub description: String,
    pub confidence:  f32,
}

#[derive(Debug, Serialize)]
pub struct TargetInfo {
    pub name: String,
    pub kind: String,
    pub host: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeployEntry {
    pub id:                   String,
    pub target:               String,
    pub adapter:              String,
    pub status:               String,
    pub artifact_fingerprint: String,
    pub duration_ms:          u64,
    pub size_bytes:           u64,
    pub deployed_at:          String,
}

#[derive(Debug, Serialize)]
pub struct SnapshotEntry {
    pub id:          String,
    pub target:      String,
    pub adapter:     String,
    pub deployed_at: String,
    pub size_bytes:  u64,
}

#[derive(Debug, Serialize)]
pub struct StatusEntry {
    pub name:       String,
    pub kind:       String,
    pub online:     bool,
    pub latency_ms: Option<u64>,
    pub host:       Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CostEntry {
    pub target:             String,
    pub upload_usd:         f64,
    pub egress_monthly_usd: f64,
    pub breakdown:          String,
}

#[derive(Debug, Serialize)]
pub struct DoctorEntry {
    pub name:    String,
    pub ok:      bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ShipResult {
    pub ok:                   bool,
    pub target:               String,
    pub url:                  Option<String>,
    pub snapshot_id:          Option<String>,
    pub artifact_fingerprint: String,
    pub size_bytes:           u64,
    pub duration_ms:          u64,
}

// â”€â”€â”€ READ commands â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tauri::command]
pub async fn detect_project(root: String) -> Result<DetectResult, String> {
    detect(Path::new(&root))
        .map(|r| DetectResult {
            adapter:     r.adapter,
            description: r.description,
            confidence:  r.confidence,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_project_root(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.project_root().map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn set_project_root(root: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = PathBuf::from(&root);
    if !path.exists() {
        return Err(format!("path does not exist: {root}"));
    }
    state.set_project_root(path);
    Ok(())
}

#[tauri::command]
pub async fn open_project_dialog(state: State<'_, AppState>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // Note: Dialog plugin returns via callback; this is handled in the frontend
    // by calling tauri-plugin-dialog directly from TypeScript.
    // This command exists as a fallback for platforms without FS access.
    Ok(state.project_root().map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn list_targets(root: String) -> Result<Vec<TargetInfo>, String> {
    let (config, _) = ArkonConfig::load(Path::new(&root))
        .map_err(|e| e.to_string())?;

    Ok(config.targets.iter().map(|(name, cfg)| {
        let (kind, host) = match cfg {
            arkon_core::config::TargetConfig::Ssh(c) =>
                ("ssh", Some(c.host.clone())),
            arkon_core::config::TargetConfig::S3(c) =>
                ("s3", c.endpoint.clone().or_else(|| Some(format!(
                    "s3.{}.amazonaws.com",
                    c.region.as_deref().unwrap_or("us-east-1")
                )))),
            arkon_core::config::TargetConfig::B2(c) =>
                ("b2", Some(c.bucket.clone())),
            arkon_core::config::TargetConfig::R2(c) =>
                ("r2", Some(c.bucket.clone())),
            arkon_core::config::TargetConfig::Webrtc(_) =>
                ("webrtc", None),
            arkon_core::config::TargetConfig::Ipfs(c) =>
                ("ipfs", c.api.clone()),
            arkon_core::config::TargetConfig::GithubPages(c) =>
                ("github-pages", Some(c.repo.clone())),
            arkon_core::config::TargetConfig::Local(c) =>
                ("local", Some(c.path.to_string_lossy().into_owned())),
        };
        TargetInfo { name: name.clone(), kind: kind.to_string(), host }
    }).collect())
}

#[tauri::command]
pub async fn get_deploy_history(limit: usize) -> Result<Vec<DeployEntry>, String> {
    let log_path = dirs::home_dir()
        .ok_or("cannot find home dir")?
        .join(".arkon/audit.log");

    if !log_path.exists() { return Ok(vec![]); }

    let raw = std::fs::read_to_string(&log_path).map_err(|e| e.to_string())?;
    Ok(raw.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<arkon_core::deploy::DeployRecord>(l).ok())
        .rev().take(limit)
        .map(|r| DeployEntry {
            id:                   r.id[..8.min(r.id.len())].to_string(),
            target:               r.target,
            adapter:              r.adapter,
            status:               format!("{:?}", r.status).to_lowercase(),
            artifact_fingerprint: r.artifact_fingerprint[..12.min(r.artifact_fingerprint.len())].to_string(),
            duration_ms:          r.duration_ms,
            size_bytes:           r.size_bytes,
            deployed_at:          r.started_at.to_rfc3339(),
        })
        .collect())
}

#[tauri::command]
pub async fn get_snapshots(
    project: String,
    target: Option<String>,
) -> Result<Vec<SnapshotEntry>, String> {
    let store   = SnapshotStore::open(&project).map_err(|e| e.to_string())?;
    let entries = store.list(target.as_deref()).map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(|e| SnapshotEntry {
        id:          e.id[..8.min(e.id.len())].to_string(),
        target:      e.target,
        adapter:     e.adapter,
        deployed_at: e.deployed_at.to_rfc3339(),
        size_bytes:  e.size_bytes,
    }).collect())
}

#[tauri::command]
pub async fn list_secrets(project: String) -> Result<Vec<String>, String> {
    Vault::open(&project)
        .and_then(|v| v.list_keys())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_secret(
    project: String,
    key: String,
    value: String,
) -> Result<(), String> {
    Vault::open(&project)
        .and_then(|v| v.set(&key, value.as_bytes()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_secret(project: String, key: String) -> Result<(), String> {
    Vault::open(&project)
        .and_then(|v| v.delete(&key))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_status(root: String) -> Result<Vec<StatusEntry>, String> {
    use std::{net::TcpStream, time::Duration};

    let (config, _) = ArkonConfig::load(Path::new(&root))
        .map_err(|e| e.to_string())?;

    Ok(config.targets.iter().map(|(name, cfg)| {
        match cfg {
            arkon_core::config::TargetConfig::Ssh(c) => {
                let addr  = format!("{}:{}", c.host, c.port.unwrap_or(22));
                let start = std::time::Instant::now();
                let ok    = TcpStream::connect_timeout(
                    &addr.parse().unwrap_or_else(|_| "0.0.0.0:22".parse().unwrap()),
                    Duration::from_secs(4),
                ).is_ok();
                StatusEntry {
                    name: name.clone(), kind: "ssh".into(), online: ok,
                    host: Some(c.host.clone()),
                    latency_ms: if ok { Some(start.elapsed().as_millis() as u64) } else { None },
                }
            }
            arkon_core::config::TargetConfig::S3(c) => {
                let host = c.endpoint.as_deref()
                    .and_then(|ep| url::Url::parse(ep).ok())
                    .and_then(|u| u.host_str().map(|h| h.to_string()))
                    .unwrap_or_else(|| format!("s3.{}.amazonaws.com",
                        c.region.as_deref().unwrap_or("us-east-1")));
                let start = std::time::Instant::now();
                let ok    = TcpStream::connect_timeout(
                    &format!("{host}:443").parse().unwrap_or_else(|_| "0.0.0.0:443".parse().unwrap()),
                    Duration::from_secs(4),
                ).is_ok();
                StatusEntry {
                    name: name.clone(), kind: "s3".into(), online: ok,
                    host: Some(host),
                    latency_ms: if ok { Some(start.elapsed().as_millis() as u64) } else { None },
                }
            }
            arkon_core::config::TargetConfig::Ipfs(c) => {
                let api  = c.api.clone().unwrap_or_else(|| "127.0.0.1:5001".into());
                let ok   = TcpStream::connect_timeout(
                    &"127.0.0.1:5001".parse().unwrap(),
                    Duration::from_secs(2),
                ).is_ok();
                StatusEntry {
                    name: name.clone(), kind: "ipfs".into(), online: ok,
                    host: Some(api), latency_ms: None,
                }
            }
            arkon_core::config::TargetConfig::GithubPages(c) => StatusEntry {
                name: name.clone(), kind: "github-pages".into(), online: true,
                host: Some(c.repo.clone()), latency_ms: None,
            },
            arkon_core::config::TargetConfig::Webrtc(_) => StatusEntry {
                name: name.clone(), kind: "webrtc".into(), online: true,
                host: None, latency_ms: None,
            },
            _ => StatusEntry {
                name: name.clone(), kind: "local".into(), online: true,
                host: None, latency_ms: None,
            },
        }
    }).collect())
}

#[tauri::command]
pub async fn doctor_check() -> Result<Vec<DoctorEntry>, String> {
    use std::process::Command;
    Ok(vec![
        ("git",    Command::new("git").arg("--version").output().is_ok()),
        ("rsync",  Command::new("rsync").arg("--version").output().is_ok()),
        ("ssh",    Command::new("ssh").arg("-V").output().is_ok()),
        ("docker", Command::new("docker").arg("info").output()
            .map(|o| o.status.success()).unwrap_or(false)),
        ("ipfs",   std::net::TcpStream::connect_timeout(
            &"127.0.0.1:5001".parse().unwrap(),
            std::time::Duration::from_secs(2)).is_ok()),
    ].into_iter().map(|(name, ok)| DoctorEntry {
        name: name.to_string(), ok,
        message: if ok { "found".into() } else { format!("{name} not found or not running") },
    }).collect())
}

// â”€â”€â”€ WRITE commands (with progress streaming) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Deploy the project to a target. Streams progress events to the frontend.
#[tauri::command]
pub async fn ship(
    root:    String,
    target:  Option<String>,
    dry_run: bool,
    on_progress: Channel<ProgressEvent>,
) -> Result<ShipResult, String> {
    let root_path = PathBuf::from(&root);

    macro_rules! emit {
        (log $msg:expr) => { let _ = on_progress.send(ProgressEvent::Log { message: $msg.to_string() }); };
        (step $s:expr, $d:expr) => { let _ = on_progress.send(ProgressEvent::Step { step: $s.to_string(), detail: $d.to_string() }); };
    }

    // 1. Load config
    let (config, config_path) = ArkonConfig::load(&root_path)
        .map_err(|e| { let _ = on_progress.send(ProgressEvent::Error { message: e.to_string() }); e.to_string() })?;
    let project_root = config_path.parent().unwrap_or(&root_path).to_path_buf();

    // 2. Detect adapter
    emit!(log "detecting project type...");
    let detection    = detect(&project_root).map_err(|e| e.to_string())?;
    let adapter_name = config.project.adapter.as_deref()
        .unwrap_or(&detection.adapter).to_string();
    emit!(step "adapter", &format!("{} ({:.0}%)", adapter_name, detection.confidence * 100.0));

    // 3. Load secrets
    let vault = Vault::open(&config.project.name).map_err(|e| e.to_string())?;
    let env   = vault.export_all()
        .map(|m| m.into_iter().map(|(k,v)| (k, v.to_string())).collect::<std::collections::HashMap<String,String>>())
        .unwrap_or_default();

    // 4. Build
    emit!(log "building artifact...");
    let registry = AdapterRegistry::default();
    let adapter  = registry.get(&adapter_name).map_err(|e| e.to_string())?;
    let ctx = BuildCtx {
        root: project_root.clone(),
        env: env.clone(),
        command_override:    config.build.command.clone(),
        output_dir_override: config.build.output_dir.clone(),
        cache_enabled:       config.project.cache,
        last_fingerprint:    String::new(),
    };
    let runner   = BuildRunner { adapter: adapter.as_ref(), ctx: &ctx };
    let artifact = runner.run().map_err(|e| e.to_string())?;

    emit!(step "artifact",
        &format!("{:.1}MB  {}", artifact.size_bytes as f64 / 1_048_576.0,
            &artifact.fingerprint[..12]));

    if dry_run {
        emit!(log "dry run â€” skipping deploy");
        let _ = on_progress.send(ProgressEvent::Success {
            message: "dry run complete".into(), url: None,
        });
        let _ = on_progress.send(ProgressEvent::Done);
        return Ok(ShipResult {
            ok: true, target: "(dry-run)".into(), url: None,
            snapshot_id: None,
            artifact_fingerprint: artifact.fingerprint,
            size_bytes: artifact.size_bytes, duration_ms: 0,
        });
    }

    // 5. Select target
    let target_name = target
        .or_else(|| config.deploy.default_target.clone())
        .ok_or_else(|| "no target specified and no default_target in arkon.toml".to_string())?;
    emit!(step "target", &target_name);

    // 6. Dispatch
    emit!(log &format!("deploying to {}...", target_name));
    let start      = std::time::Instant::now();
    let deploy_ctx = DeployCtx {
        project_name: config.project.name.clone(),
        target_name:  target_name.clone(),
        project_root: project_root.clone(),
        dry_run:      false,
        env,
    };

    let dispatcher = Dispatcher::default();
    let record = dispatcher
        .dispatch(&artifact, &target_name, &deploy_ctx, &config)
        .map_err(|e| { let _ = on_progress.send(ProgressEvent::Error { message: e.to_string() }); e.to_string() })?;

    // 7. Save snapshot
    let store = SnapshotStore::open(&config.project.name).ok();
    let snapshot_id = store.as_ref()
        .and_then(|s| s.save(&artifact, &record, &adapter_name).ok())
        .map(|snap| snap.id[..8].to_string());

    let url = record.notes.clone();
    let ms  = start.elapsed().as_millis() as u64;

    emit!(log &format!("deployed in {ms}ms"));
    let _ = on_progress.send(ProgressEvent::Success {
        message: format!("deployed to {target_name} in {ms}ms"),
        url: url.clone(),
    });
    let _ = on_progress.send(ProgressEvent::Done);

    Ok(ShipResult {
        ok: true,
        target: target_name,
        url,
        snapshot_id,
        artifact_fingerprint: artifact.fingerprint,
        size_bytes: artifact.size_bytes,
        duration_ms: ms,
    })
}

/// Roll back to a previous snapshot.
#[tauri::command]
pub async fn rollback(
    root:        String,
    query:       String,
    target:      Option<String>,
    on_progress: Channel<ProgressEvent>,
) -> Result<ShipResult, String> {
    let root_path = PathBuf::from(&root);
    let (config, config_path) = ArkonConfig::load(&root_path)
        .map_err(|e| e.to_string())?;
    let project_root = config_path.parent().unwrap_or(&root_path).to_path_buf();

    let _ = on_progress.send(ProgressEvent::Log {
        message: format!("finding snapshot '{query}'..."),
    });

    let store    = SnapshotStore::open(&config.project.name).map_err(|e| e.to_string())?;
    let snapshot = store.find(&query, target.as_deref()).map_err(|e| e.to_string())?;

    let _ = on_progress.send(ProgressEvent::Step {
        step:   "snapshot".into(),
        detail: format!("{}  {}  {}", &snapshot.id[..8], snapshot.target,
            snapshot.deployed_at.format("%Y-%m-%d %H:%M")),
    });

    let artifact = SnapshotStore::reconstruct_artifact(
        &snapshot, &project_root, &snapshot.adapter,
    );

    let vault = Vault::open(&config.project.name).map_err(|e| e.to_string())?;
    let env   = vault.export_all().map(|m| m.into_iter().map(|(k,v)| (k, v.to_string())).collect::<std::collections::HashMap<String,String>>()).unwrap_or_default();

    let deploy_ctx = DeployCtx {
        project_name: config.project.name.clone(),
        target_name:  snapshot.target.clone(),
        project_root: project_root.clone(),
        dry_run:      false,
        env,
    };

    let _ = on_progress.send(ProgressEvent::Log {
        message: format!("re-deploying to {}...", snapshot.target),
    });

    let start      = std::time::Instant::now();
    let dispatcher = Dispatcher::default();
    let record     = dispatcher
        .dispatch(&artifact, &snapshot.target, &deploy_ctx, &config)
        .map_err(|e| e.to_string())?;

    store.save(&artifact, &record, &snapshot.adapter).ok();

    let ms = start.elapsed().as_millis() as u64;
    let _ = on_progress.send(ProgressEvent::Success {
        message: format!("rolled back to {} in {ms}ms", &snapshot.id[..8]),
        url: record.notes.clone(),
    });
    let _ = on_progress.send(ProgressEvent::Done);

    Ok(ShipResult {
        ok: true,
        target: snapshot.target,
        url: record.notes,
        snapshot_id: Some(snapshot.id[..8].to_string()),
        artifact_fingerprint: snapshot.artifact_fingerprint,
        size_bytes: snapshot.size_bytes,
        duration_ms: ms,
    })
}

/// Promote latest snapshot from source target to destination.
#[tauri::command]
pub async fn promote(
    root:        String,
    from:        String,
    to:          String,
    on_progress: Channel<ProgressEvent>,
) -> Result<ShipResult, String> {
    let root_path = PathBuf::from(&root);
    let (config, config_path) = ArkonConfig::load(&root_path)
        .map_err(|e| e.to_string())?;
    let project_root = config_path.parent().unwrap_or(&root_path).to_path_buf();

    let _ = on_progress.send(ProgressEvent::Log {
        message: format!("promoting {from} â†’ {to}..."),
    });

    let store    = SnapshotStore::open(&config.project.name).map_err(|e| e.to_string())?;
    let snapshot = store.find("latest", Some(&from))
        .map_err(|_| format!("no snapshot found for '{from}' â€” deploy to '{from}' first"))?;

    let _ = on_progress.send(ProgressEvent::Step {
        step:   "snapshot".into(),
        detail: format!("{}  {}MB", &snapshot.id[..8],
            snapshot.size_bytes / 1_048_576),
    });

    let artifact = SnapshotStore::reconstruct_artifact(
        &snapshot, &project_root, &snapshot.adapter,
    );

    let vault = Vault::open(&config.project.name).map_err(|e| e.to_string())?;
    let env   = vault.export_all().map(|m| m.into_iter().map(|(k,v)| (k, v.to_string())).collect::<std::collections::HashMap<String,String>>()).unwrap_or_default();

    let deploy_ctx = DeployCtx {
        project_name: config.project.name.clone(),
        target_name:  to.clone(),
        project_root: project_root.clone(),
        dry_run:      false,
        env,
    };

    let start      = std::time::Instant::now();
    let dispatcher = Dispatcher::default();
    let record     = dispatcher
        .dispatch(&artifact, &to, &deploy_ctx, &config)
        .map_err(|e| e.to_string())?;

    store.save(&artifact, &record, &snapshot.adapter).ok();

    let ms = start.elapsed().as_millis() as u64;
    let _ = on_progress.send(ProgressEvent::Success {
        message: format!("promoted {from} â†’ {to} in {ms}ms"),
        url: record.notes.clone(),
    });
    let _ = on_progress.send(ProgressEvent::Done);

    Ok(ShipResult {
        ok: true,
        target: to,
        url: record.notes,
        snapshot_id: Some(snapshot.id[..8].to_string()),
        artifact_fingerprint: snapshot.artifact_fingerprint,
        size_bytes: snapshot.size_bytes,
        duration_ms: ms,
    })
}

/// Real cost estimate â€” builds artifact then calls cost_estimate per target.
#[tauri::command]
pub async fn get_cost_estimate(root: String) -> Result<Vec<CostEntry>, String> {
    let root_path = PathBuf::from(&root);
    let (config, config_path) = ArkonConfig::load(&root_path)
        .map_err(|e| e.to_string())?;
    let project_root = config_path.parent().unwrap_or(&root_path).to_path_buf();

    let detection    = detect(&project_root).map_err(|e| e.to_string())?;
    let adapter_name = config.project.adapter.as_deref()
        .unwrap_or(&detection.adapter).to_string();

    let registry = AdapterRegistry::default();
    let adapter  = registry.get(&adapter_name).map_err(|e| e.to_string())?;
    let vault    = Vault::open(&config.project.name).map_err(|e| e.to_string())?;
    let env      = vault.export_all().map(|m| m.into_iter().map(|(k,v)| (k, v.to_string())).collect::<std::collections::HashMap<String,String>>()).unwrap_or_default();

    let ctx = BuildCtx {
        root: project_root,
        env,
        command_override:    config.build.command.clone(),
        output_dir_override: config.build.output_dir.clone(),
        cache_enabled:       true,
        last_fingerprint:    String::new(),
    };

    let runner   = BuildRunner { adapter: adapter.as_ref(), ctx: &ctx };
    let artifact = runner.run().map_err(|e| e.to_string())?;

    let dispatcher = Dispatcher::default();
    Ok(config.targets.iter().map(|(name, cfg)| {
        let hint = dispatcher
            .target_for(cfg)
            .and_then(|t| t.cost_estimate(&artifact, cfg))
            .unwrap_or_else(arkon_core::runtime::CostHint::free);
        CostEntry {
            target:             name.clone(),
            upload_usd:         hint.upload_usd,
            egress_monthly_usd: hint.egress_monthly_usd,
            breakdown:          hint.breakdown,
        }
    }).collect())
}
