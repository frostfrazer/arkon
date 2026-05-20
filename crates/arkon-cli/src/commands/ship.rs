use arkon_adapters::{AdapterRegistry, BuildCtx, BuildRunner};
use arkon_core::{
    config::ArkonConfig,
    deploy::{DeployCtx, DeployStatus},
    error::{ArkonError, Result},
};
use arkon_detector::detect;
use arkon_secrets::Vault;
use arkon_store::SnapshotStore;
use arkon_targets::Dispatcher;
use dialoguer::{theme::ColorfulTheme, Select};
use std::path::Path;

pub async fn run(
    root: &Path,
    target_override: Option<&str>,
    yes: bool,
    dry_run: bool,
) -> Result<()> {
    // â”€â”€ 1. Load config â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let (config, config_path) = ArkonConfig::load(root)?;
    let project_root = config_path.parent().unwrap_or(root).to_path_buf();
    crate::print::step("config", &config_path.display().to_string());

    // â”€â”€ 2. Detect project â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let detection = detect(&project_root)?;
    detection.print_summary();

    // Override adapter if forced in config
    let adapter_name = config.project.adapter.as_deref()
        .unwrap_or(&detection.adapter);

    // â”€â”€ 3. Load adapter â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let registry = AdapterRegistry::default();
    let adapter = registry.get(adapter_name)?;

    // â”€â”€ 4. Load secrets from vault â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let vault = Vault::open(&config.project.name)?;
    let env_map = vault.export_all()
        .map(|m| m.into_iter().map(|(k,v)| (k, v.to_string())).collect::<std::collections::HashMap<String,String>>())
        .unwrap_or_default();

    // â”€â”€ 5. Build â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let ctx = BuildCtx {
        root: project_root.clone(),
        env: env_map.clone(),
        command_override: config.build.command.clone(),
        output_dir_override: config.build.output_dir.clone(),
        cache_enabled: config.project.cache,
        last_fingerprint: load_last_fingerprint(&config.project.name, adapter_name),
    };

    crate::print::info(&format!("building with adapter \x1b[1m{}\x1b[0m", adapter.name()));
    let runner = BuildRunner { adapter: adapter.as_ref(), ctx: &ctx };
    let artifact = runner.run()?;

    crate::print::step(
        "artifact",
        &format!(
            "{}  [{:.1} MB]  fingerprint {}",
            artifact.name,
            artifact.size_bytes as f64 / 1_048_576.0,
            &artifact.fingerprint[..12],
        ),
    );

    if dry_run {
        crate::print::warn("dry run â€” skipping deploy");
        return Ok(());
    }

    // â”€â”€ 6. Select target â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let target_name = if let Some(t) = target_override {
        t.to_string()
    } else if let Some(default) = &config.deploy.default_target {
        if yes {
            default.clone()
        } else {
            prompt_target(&config)?
        }
    } else {
        prompt_target(&config)?
    };

    // â”€â”€ 7. Cost estimate â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let target_cfg = config.target(&target_name)?;
    let cost = adapter.cost_estimate(&artifact, target_cfg);
    if let Some(ref hint) = cost {
        let threshold = config.deploy.cost_threshold_usd.unwrap_or(0.01);
        if config.deploy.confirm_cost && hint.total() > threshold {
            crate::print::warn(&format!(
                "estimated cost: ${:.4} upload + ${:.4}/mo egress â€” {}",
                hint.upload_usd, hint.egress_monthly_usd, hint.breakdown
            ));
            if !yes && !confirm("proceed with deploy?")? {
                return Err(ArkonError::Other(anyhow::anyhow!("deploy cancelled by user")));
            }
        }
    }

    // â”€â”€ 8. Deploy â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let deploy_ctx = DeployCtx {
        project_name:  config.project.name.clone(),
        target_name:   target_name.clone(),
        project_root:  project_root.clone(),
        dry_run,
        env: env_map.clone(),
    };

    let dispatcher = Dispatcher::default();
    let record = dispatcher.dispatch(&artifact, &target_name, &deploy_ctx, &config)?;

    // â”€â”€ 9. Save snapshot (enables rollback / promote) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let store = SnapshotStore::open(&config.project.name)?;
    if let Err(e) = store.save(&artifact, &record, adapter.name()) {
        crate::print::warn(&format!("snapshot save failed (non-fatal): {e}"));
    }

    // â”€â”€ 10. Save fingerprint for next cache check â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    save_fingerprint(&config.project.name, adapter_name, &artifact.fingerprint);

    // â”€â”€ 11. Emit result (human or JSON) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let snapshot_id = store
        .list(Some(&target_name))
        .ok()
        .and_then(|e| e.into_iter().next())
        .map(|e| e.id[..8].to_string());

    if crate::json_output::is_enabled() {
        let result = crate::json_output::JsonResult {
            ok:                   true,
            version:              env!("CARGO_PKG_VERSION").to_string(),
            command:              "ship".to_string(),
            project:              config.project.name.clone(),
            adapter:              adapter.name().to_string(),
            target:               target_name.clone(),
            url:                  record.notes.clone(),
            snapshot_id,
            artifact_fingerprint: artifact.fingerprint.clone(),
            size_bytes:           Some(artifact.size_bytes),
            duration_ms:          Some(record.duration_ms),
            deployed_at:          record.finished_at.to_rfc3339(),
            ..Default::default()
        };
        crate::json_output::output_success(&result);
    } else {
        crate::print::separator();
        crate::print::success(&format!(
            "deploy complete  [{:.1}s]",
            record.duration_ms as f64 / 1000.0
        ));
        if let Some(url) = &record.notes {
            crate::print::url("live at", url);
        }
        if let Some(ref sid) = snapshot_id {
            crate::print::step("snapshot", sid);
        }
        println!();
    }
    Ok(())
}

fn prompt_target(config: &ArkonConfig) -> Result<String> {
    let names: Vec<&String> = config.targets.keys().collect();
    if names.is_empty() {
        return Err(ArkonError::ConfigError(
            "no targets defined in arkon.toml. Add a [targets.*] section.".into(),
        ));
    }
    if names.len() == 1 {
        return Ok(names[0].to_string());
    }
    let options: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("  deploy to")
        .items(&options)
        .default(0)
        .interact()
        .map_err(|e| ArkonError::Other(e.into()))?;
    Ok(options[idx].clone())
}

fn confirm(prompt: &str) -> Result<bool> {
    dialoguer::Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("  {prompt}"))
        .default(true)
        .interact()
        .map_err(|e| ArkonError::Other(e.into()))
}

fn fingerprint_cache_path(project: &str, adapter: &str) -> std::path::PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join(".arkon").join("cache").join(format!("{}-{}.fp", project, adapter))
}

fn load_last_fingerprint(project: &str, adapter: &str) -> String {
    std::fs::read_to_string(fingerprint_cache_path(project, adapter))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn save_fingerprint(project: &str, adapter: &str, fp: &str) {
    let path = fingerprint_cache_path(project, adapter);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, fp);
}
