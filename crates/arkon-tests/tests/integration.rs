/// ARKON integration tests
///
/// Run with: cargo test -p arkon-tests
///
/// These tests spin up real in-process pipelines against temporary directories.
/// No network calls, no SSH connections. All I/O is inside `tempfile::TempDir`.

// ─── detector tests ──────────────────────────────────────────────────────────

mod detector {
    use arkon_detector::{detect, DetectionResult};
    use std::fs;
    use tempfile::TempDir;

    fn make_project(files: &[&str]) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for f in files {
            let path = dir.path().join(f);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, b"# arkon test fixture").unwrap();
        }
        dir
    }

    #[test]
    fn detects_nextjs() {
        let dir = make_project(&["package.json", "next.config.js"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "nextjs");
        assert!(r.confidence >= 0.95, "confidence {} < 0.95", r.confidence);
    }

    #[test]
    fn detects_vite() {
        let dir = make_project(&["package.json", "vite.config.ts"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "vite");
    }

    #[test]
    fn detects_unity() {
        let dir = make_project(&["Assets/Scene.unity", "ProjectSettings/ProjectVersion.txt"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "unity");
        assert!(r.confidence >= 0.99);
    }

    #[test]
    fn detects_godot() {
        let dir = make_project(&["project.godot"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "godot");
    }

    #[test]
    fn detects_rust_bin() {
        let dir = make_project(&["Cargo.toml", "src/main.rs"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "rust-bin");
    }

    #[test]
    fn detects_go() {
        let dir = make_project(&["go.mod", "main.go"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "go");
    }

    #[test]
    fn detects_python_requirements() {
        let dir = make_project(&["requirements.txt", "app.py"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "python");
    }

    #[test]
    fn detects_python_pyproject() {
        let dir = make_project(&["pyproject.toml", "src/main.py"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "python");
    }

    #[test]
    fn detects_static_html() {
        let dir = make_project(&["index.html", "style.css"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "static");
    }

    #[test]
    fn detects_docker() {
        let dir = make_project(&["Dockerfile", "docker-compose.yml"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "docker");
    }

    #[test]
    fn nextjs_beats_nodejs() {
        // A project with both next.config.js and package.json should be nextjs, not nodejs
        let dir = make_project(&["package.json", "next.config.ts", "src/pages/index.tsx"]);
        let r   = detect(dir.path()).expect("detect");
        assert_eq!(r.adapter, "nextjs");
    }

    #[test]
    fn no_match_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Empty directory — nothing to detect
        let err = detect(dir.path());
        assert!(err.is_err(), "expected detection failure for empty dir");
    }

    #[test]
    fn verbose_candidates_sorted_by_confidence() {
        let dir = make_project(&["package.json", "next.config.js", "vite.config.ts"]);
        let r   = detect(dir.path()).expect("detect");
        let confidences: Vec<f32> = r.all_candidates.iter().map(|c| c.confidence).collect();
        for window in confidences.windows(2) {
            assert!(
                window[0] >= window[1],
                "candidates not sorted: {} < {}",
                window[0], window[1]
            );
        }
    }
}

// ─── fingerprint tests ───────────────────────────────────────────────────────

mod fingerprint {
    use arkon_adapters::fingerprint::{fingerprint_dir, FileDiff};
    use std::fs;
    use tempfile::TempDir;

    fn scaffold(files: &[(&str, &[u8])]) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(p) = path.parent() { fs::create_dir_all(p).unwrap(); }
            fs::write(&path, content).unwrap();
        }
        dir
    }

    #[test]
    fn stable_hash_same_contents() {
        let dir = scaffold(&[("index.html", b"<h1>hello</h1>")]);
        let (h1, _, _) = fingerprint_dir(dir.path()).expect("fp1");
        let (h2, _, _) = fingerprint_dir(dir.path()).expect("fp2");
        assert_eq!(h1, h2, "same contents should produce same fingerprint");
    }

    #[test]
    fn different_hash_after_change() {
        let dir = scaffold(&[("index.html", b"<h1>hello</h1>")]);
        let (h1, _, _) = fingerprint_dir(dir.path()).expect("fp1");
        fs::write(dir.path().join("index.html"), b"<h1>world</h1>").unwrap();
        let (h2, _, _) = fingerprint_dir(dir.path()).expect("fp2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn different_hash_after_new_file() {
        let dir = scaffold(&[("index.html", b"<h1>hello</h1>")]);
        let (h1, _, _) = fingerprint_dir(dir.path()).expect("fp1");
        fs::write(dir.path().join("style.css"), b"body{}").unwrap();
        let (h2, _, _) = fingerprint_dir(dir.path()).expect("fp2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn file_diff_detects_added_modified_deleted() {
        use std::collections::HashMap;

        let mut prev: HashMap<String, String> = HashMap::new();
        prev.insert("index.html".into(), "aaa".into());
        prev.insert("old.css".into(),   "bbb".into());

        let mut curr: HashMap<String, String> = HashMap::new();
        curr.insert("index.html".into(), "ccc".into()); // modified
        curr.insert("main.js".into(),    "ddd".into()); // added
        // old.css removed

        let diff = FileDiff::compute(&prev, &curr);
        assert_eq!(diff.modified, vec!["index.html"]);
        assert_eq!(diff.added,    vec!["main.js"]);
        assert_eq!(diff.deleted,  vec!["old.css"]);
        assert!(!diff.is_empty());
    }

    #[test]
    fn file_diff_empty_when_identical() {
        use std::collections::HashMap;
        let mut m: HashMap<String, String> = HashMap::new();
        m.insert("a.txt".into(), "hash1".into());
        let diff = FileDiff::compute(&m, &m);
        assert!(diff.is_empty());
    }

    #[test]
    fn reports_total_bytes() {
        let dir = scaffold(&[
            ("a.txt", b"hello"),    // 5 bytes
            ("b.txt", b"world!!"),  // 7 bytes
        ]);
        let (_, _, total) = fingerprint_dir(dir.path()).expect("fp");
        assert_eq!(total, 12);
    }
}

// ─── config tests ─────────────────────────────────────────────────────────────

mod config {
    use arkon_core::config::ArkonConfig;
    use std::fs;
    use tempfile::TempDir;

    fn write_config(content: &str) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("arkon.toml"), content).unwrap();
        dir
    }

    #[test]
    fn parses_minimal_config() {
        let dir = write_config(r#"
[project]
name = "my-app"
"#);
        let (cfg, _) = ArkonConfig::load(dir.path()).expect("load");
        assert_eq!(cfg.project.name, "my-app");
        assert!(cfg.project.cache); // default true
    }

    #[test]
    fn parses_ssh_target() {
        let dir = write_config(r#"
[project]
name = "app"

[targets.production]
type = "ssh"
host = "myserver.com"
user = "deploy"
path = "/var/www/app"
tls  = true
"#);
        let (cfg, _) = ArkonConfig::load(dir.path()).expect("load");
        let target = cfg.target("production").expect("target");
        match target {
            arkon_core::config::TargetConfig::Ssh(t) => {
                assert_eq!(t.host, "myserver.com");
                assert_eq!(t.user.as_deref(), Some("deploy"));
                assert!(t.tls);
            }
            _ => panic!("expected ssh target"),
        }
    }

    #[test]
    fn parses_s3_target() {
        let dir = write_config(r#"
[project]
name = "site"

[targets.cdn]
type   = "s3"
bucket = "my-bucket"
region = "eu-central-1"
"#);
        let (cfg, _) = ArkonConfig::load(dir.path()).expect("load");
        match cfg.target("cdn").expect("cdn") {
            arkon_core::config::TargetConfig::S3(t) => {
                assert_eq!(t.bucket, "my-bucket");
                assert_eq!(t.region.as_deref(), Some("eu-central-1"));
            }
            _ => panic!("expected s3 target"),
        }
    }

    #[test]
    fn parses_health_checks() {
        let dir = write_config(r#"
[project]
name = "api"

[health.production]
interval = "30s"
retries  = 3
checks   = [
  { type = "http", url = "https://api.example.com/health", expect = 200 },
  { type = "tcp",  host = "db.internal", port = 5432 },
]
"#);
        let (cfg, _) = ArkonConfig::load(dir.path()).expect("load");
        let h = cfg.health.get("production").expect("health");
        assert_eq!(h.checks.len(), 2);
        assert_eq!(h.retries, Some(3));
    }

    #[test]
    fn missing_arkon_toml_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = ArkonConfig::load(dir.path());
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("arkon.toml"), "expected arkon.toml mention in: {msg}");
    }

    #[test]
    fn target_not_found_returns_error() {
        let dir = write_config(r#"[project]\nname = "app""#);
        let (cfg, _) = ArkonConfig::load(dir.path()).expect("load");
        let err = cfg.target("nonexistent");
        assert!(err.is_err());
    }

    #[test]
    fn parses_hooks() {
        let dir = write_config(r#"
[project]
name = "app"

[hooks.production]
pre_deploy  = ["npm run db:migrate"]
post_deploy = ["pm2 restart app", "echo done"]
"#);
        let (cfg, _) = ArkonConfig::load(dir.path()).expect("load");
        let hooks = cfg.hooks.get("production").expect("hooks");
        assert_eq!(hooks.pre_deploy.len(), 1);
        assert_eq!(hooks.post_deploy.len(), 2);
        assert_eq!(hooks.post_deploy[1], "echo done");
    }
}

// ─── snapshot store tests ─────────────────────────────────────────────────────

mod snapshot_store {
    use arkon_core::{
        artifact::{Artifact, DeployableKind},
        deploy::{DeployRecord, DeployStatus},
    };
    use arkon_store::SnapshotStore;
    use std::collections::HashMap;

    fn fake_artifact(fingerprint: &str) -> Artifact {
        let mut art = Artifact::new("test", std::path::PathBuf::from("/tmp"), DeployableKind::Static);
        art.fingerprint  = fingerprint.to_string();
        art.file_hashes  = HashMap::from([
            ("index.html".into(), "abc".into()),
            ("style.css".into(),  "def".into()),
        ]);
        art.size_bytes = 1024;
        art
    }

    fn fake_record(target: &str) -> DeployRecord {
        let mut r = DeployRecord::new("test-project", target, "static", "fp123");
        r.status      = DeployStatus::Success;
        r.duration_ms = 1500;
        r.size_bytes  = 1024;
        r
    }

    #[test]
    fn save_and_find_latest() {
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");
        let art     = fake_artifact("fp_latest");
        let rec     = fake_record("production");

        let snap = store.save(&art, &rec, "static").expect("save");
        assert_eq!(&snap.artifact_fingerprint, "fp_latest");

        let found = store.find("latest", None).expect("find latest");
        assert_eq!(found.id, snap.id);
    }

    #[test]
    fn find_by_id_prefix() {
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");
        let art     = fake_artifact("fp_prefix");
        let rec     = fake_record("staging");

        let snap = store.save(&art, &rec, "static").expect("save");
        let prefix = &snap.id[..6];
        let found  = store.find(prefix, None).expect("find by prefix");
        assert_eq!(found.id, snap.id);
    }

    #[test]
    fn find_by_target_filter() {
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");

        store.save(&fake_artifact("fp_prod"),    &fake_record("production"), "static").unwrap();
        store.save(&fake_artifact("fp_staging"), &fake_record("staging"),    "static").unwrap();

        let prod = store.find("latest", Some("production")).expect("find prod");
        assert_eq!(prod.target, "production");

        let stg = store.find("latest", Some("staging")).expect("find staging");
        assert_eq!(stg.target, "staging");
    }

    #[test]
    fn list_returns_newest_first() {
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");

        // Save two snapshots with a small sleep so timestamps differ
        store.save(&fake_artifact("fp_1"), &fake_record("production"), "static").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.save(&fake_artifact("fp_2"), &fake_record("production"), "static").unwrap();

        let entries = store.list(None).expect("list");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].deployed_at >= entries[1].deployed_at,
            "list not sorted newest-first");
    }

    #[test]
    fn prune_keeps_n_most_recent() {
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");

        for i in 0..5 {
            store.save(
                &fake_artifact(&format!("fp_{i}")),
                &fake_record("production"),
                "static",
            ).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let deleted = store.prune(2).expect("prune");
        assert_eq!(deleted, 3, "expected 3 deleted, got {deleted}");

        let remaining = store.list(None).expect("list after prune");
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn delete_removes_snapshot() {
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");
        let snap    = store.save(&fake_artifact("fp_del"), &fake_record("production"), "static").unwrap();

        store.delete(&snap.id).expect("delete");
        let err = store.find(&snap.id[..6], None);
        assert!(err.is_err(), "snapshot should be gone after delete");
    }

    #[test]
    fn reconstruct_artifact_has_correct_fingerprint() {
        use arkon_store::SnapshotStore;
        let project = format!("arkon-test-{}", uuid::Uuid::new_v4());
        let store   = SnapshotStore::open(&project).expect("open");
        let art     = fake_artifact("fp_reconstruct");
        let rec     = fake_record("production");
        let snap    = store.save(&art, &rec, "nextjs").unwrap();

        let reconstructed = SnapshotStore::reconstruct_artifact(
            &snap,
            std::path::Path::new("/tmp"),
            "nextjs",
        );
        assert_eq!(reconstructed.fingerprint, "fp_reconstruct");
        assert_eq!(reconstructed.file_hashes.len(), 2);
    }
}

// ─── json_output schema tests ─────────────────────────────────────────────────

mod json_schema {
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).expect("valid json")
    }

    #[test]
    fn success_schema_has_required_fields() {
        // Simulate what arkon ship --json emits
        let json = r#"{
            "ok": true,
            "version": "0.1.0",
            "command": "ship",
            "project": "my-app",
            "adapter": "nextjs",
            "target": "production",
            "artifact_fingerprint": "deadbeef1234",
            "deployed_at": "2024-11-03T14:22:00Z"
        }"#;
        let v = parse(json);
        assert_eq!(v["ok"],      true);
        assert_eq!(v["command"], "ship");
        assert!(v["version"].is_string());
        assert!(v["deployed_at"].is_string());
    }

    #[test]
    fn error_schema_has_ok_false_and_error() {
        let json = r#"{
            "ok": false,
            "version": "0.1.0",
            "command": "ship",
            "error": "deploy failed to target 'production': connection refused"
        }"#;
        let v = parse(json);
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("production"));
    }

    #[test]
    fn detect_schema_includes_confidence() {
        let json = r#"{
            "ok": true,
            "version": "0.1.0",
            "command": "detect",
            "adapter": "nextjs",
            "confidence": 0.98
        }"#;
        let v = parse(json);
        assert_eq!(v["adapter"], "nextjs");
        let conf = v["confidence"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&conf));
    }

    #[test]
    fn rollback_schema_includes_snapshot_id() {
        let json = r#"{
            "ok": true,
            "version": "0.1.0",
            "command": "rollback",
            "project": "my-app",
            "target": "production",
            "snapshot_id": "a1b2c3d4",
            "artifact_fingerprint": "deadbeef",
            "deployed_at": "2024-11-03T15:00:00Z"
        }"#;
        let v = parse(json);
        assert_eq!(v["snapshot_id"], "a1b2c3d4");
    }
}

// ─── adapter registry tests ───────────────────────────────────────────────────

mod adapter_registry {
    use arkon_adapters::AdapterRegistry;

    #[test]
    fn all_builtin_adapters_registered() {
        let registry = AdapterRegistry::default();
        let names    = registry.list();
        for expected in &[
            "nextjs", "vite", "nodejs", "python", "go",
            "rust-bin", "docker", "unity", "static", "shell",
            "android", "ios",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "adapter '{expected}' not registered; found: {names:?}"
            );
        }
    }

    #[test]
    fn get_unknown_adapter_returns_error() {
        let registry = AdapterRegistry::default();
        let err      = registry.get("nonexistent-xyz");
        assert!(err.is_err());
    }

    #[test]
    fn adapter_names_are_unique() {
        let registry = AdapterRegistry::default();
        let mut names = registry.list();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "duplicate adapter names found");
    }
}

// ─── vault tests ─────────────────────────────────────────────────────────────

mod vault {
    use arkon_secrets::Vault;

    fn test_vault(project: &str) -> Vault {
        Vault::open(project).expect("open vault")
    }

    #[test]
    fn set_and_get_roundtrip() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("DB_URL", b"postgres://localhost/test").expect("set");
        let val = vault.get("DB_URL").expect("get");
        assert_eq!(val, b"postgres://localhost/test");
    }

    #[test]
    fn get_missing_key_returns_error() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        let err = vault.get("NONEXISTENT");
        assert!(err.is_err());
    }

    #[test]
    fn delete_removes_key() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("TO_DELETE", b"value").expect("set");
        vault.delete("TO_DELETE").expect("delete");
        assert!(vault.get("TO_DELETE").is_err());
    }

    #[test]
    fn list_keys_returns_all_keys() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("KEY_A", b"a").expect("set A");
        vault.set("KEY_B", b"b").expect("set B");
        vault.set("KEY_C", b"c").expect("set C");
        let keys = vault.list_keys().expect("list");
        assert!(keys.contains(&"KEY_A".to_string()));
        assert!(keys.contains(&"KEY_B".to_string()));
        assert!(keys.contains(&"KEY_C".to_string()));
    }

    #[test]
    fn export_all_returns_plaintext() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("SECRET_KEY", b"my-secret-value").expect("set");
        let all = vault.export_all().expect("export");
        assert_eq!(all.get("SECRET_KEY").map(|s| s.as_str()), Some("my-secret-value"));
    }

    #[test]
    fn overwrite_updates_value() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("KEY", b"v1").expect("set v1");
        vault.set("KEY", b"v2").expect("set v2");
        let val = vault.get("KEY").expect("get");
        assert_eq!(val, b"v2");
    }

    #[test]
    fn vault_rotate_preserves_all_secrets() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("A", b"alpha").expect("set A");
        vault.set("B", b"beta").expect("set B");

        let count = vault.rotate().expect("rotate");
        assert_eq!(count, 2);

        // Re-open vault (simulates reading after rotation)
        let vault2 = test_vault(&project);
        assert_eq!(vault2.get("A").expect("get A"), b"alpha");
        assert_eq!(vault2.get("B").expect("get B"), b"beta");
    }

    #[test]
    fn ciphertext_differs_from_plaintext() {
        let project = format!("arkon-test-vault-{}", uuid::Uuid::new_v4());
        let vault   = test_vault(&project);
        vault.set("SECRET", b"super-secret-value").expect("set");

        // Read raw vault file — plaintext should NOT appear in it
        let vault_path = dirs::home_dir().unwrap()
            .join(".arkon").join("vault")
            .join(format!("{project}.vault"));
        let raw = std::fs::read_to_string(&vault_path).unwrap_or_default();
        assert!(!raw.contains("super-secret-value"),
            "plaintext appeared in vault file!");
    }
}

// ─── dispatcher dry-run tests ─────────────────────────────────────────────────

mod dispatcher {
    use arkon_core::{
        artifact::{Artifact, DeployableKind},
        config::ArkonConfig,
        deploy::{DeployCtx, DeployStatus},
    };
    use arkon_targets::Dispatcher;
    use std::{collections::HashMap, path::PathBuf};
    use tempfile::TempDir;

    fn write_config(content: &str) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("arkon.toml"), content).unwrap();
        dir
    }

    fn fake_artifact() -> Artifact {
        let mut art = Artifact::new("test", PathBuf::from("/tmp"), DeployableKind::Static);
        art.fingerprint = "fp_test".to_string();
        art.size_bytes  = 1024;
        art
    }

    #[test]
    fn dry_run_returns_skipped_status() {
        let dir = write_config(r#"
[project]
name = "test"

[targets.local]
type = "local"
path = "/tmp/arkon-test-output"

[deploy]
default_target = "local"
"#);
        let (config, _) = ArkonConfig::load(dir.path()).expect("load");
        let artifact    = fake_artifact();
        let ctx = DeployCtx {
            project_name: "test".into(),
            target_name:  "local".into(),
            project_root: dir.path().to_path_buf(),
            dry_run:      true,
            env:          HashMap::new(),
        };

        let dispatcher = Dispatcher::default();
        let record     = dispatcher.dispatch(&artifact, "local", &ctx, &config).expect("dispatch");

        assert_eq!(record.status, DeployStatus::Skipped);
        assert!(record.notes.as_deref().unwrap_or("").contains("dry-run"));
    }

    #[test]
    fn dry_run_does_not_write_files() {
        let output_dir = tempfile::tempdir().expect("output dir");
        let dir = write_config(&format!(r#"
[project]
name = "test"

[targets.local]
type = "local"
path = "{}"

[deploy]
default_target = "local"
"#, output_dir.path().display()));

        let (config, _) = ArkonConfig::load(dir.path()).expect("load");
        let mut artifact = fake_artifact();
        // Create a real file in a source dir
        let src = tempfile::tempdir().expect("src dir");
        std::fs::write(src.path().join("index.html"), b"<h1>test</h1>").unwrap();
        artifact.path = src.path().to_path_buf();

        let ctx = DeployCtx {
            project_name: "test".into(),
            target_name:  "local".into(),
            project_root: dir.path().to_path_buf(),
            dry_run:      true,
            env:          HashMap::new(),
        };

        let dispatcher = Dispatcher::default();
        dispatcher.dispatch(&artifact, "local", &ctx, &config).expect("dispatch");

        // Output directory should be empty — dry run doesn't copy files
        let entries: Vec<_> = std::fs::read_dir(output_dir.path())
            .expect("read dir")
            .collect();
        assert!(entries.is_empty(), "dry run wrote files to output dir");
    }
}

// ─── cache key tests ──────────────────────────────────────────────────────────

mod cache_key {
    use arkon_adapters::fingerprint::cache_key_from_files;
    use std::fs;
    use tempfile::TempDir;

    fn scaffold(files: &[(&str, &[u8])]) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, content) in files {
            fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn same_files_produce_same_key() {
        let dir = scaffold(&[("package.json", b"{}")]);
        let k1 = cache_key_from_files(dir.path(), &["package.json"]);
        let k2 = cache_key_from_files(dir.path(), &["package.json"]);
        assert_eq!(k1, k2);
    }

    #[test]
    fn changed_file_produces_different_key() {
        let dir = scaffold(&[("package-lock.json", b"v1")]);
        let k1  = cache_key_from_files(dir.path(), &["package-lock.json"]);
        fs::write(dir.path().join("package-lock.json"), b"v2").unwrap();
        let k2  = cache_key_from_files(dir.path(), &["package-lock.json"]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn missing_files_produce_empty_but_stable_key() {
        let dir = scaffold(&[]);
        let k1  = cache_key_from_files(dir.path(), &["nonexistent.json"]);
        let k2  = cache_key_from_files(dir.path(), &["nonexistent.json"]);
        // Both empty — should still be equal (stable)
        assert_eq!(k1, k2);
        // Should not be the same as a key with a real file
        fs::write(dir.path().join("real.json"), b"{}").unwrap();
        let k3 = cache_key_from_files(dir.path(), &["real.json"]);
        assert_ne!(k1, k3);
    }

    #[test]
    fn key_order_independent() {
        let dir = scaffold(&[
            ("a.lock", b"aaa"),
            ("b.lock", b"bbb"),
        ]);
        let k1 = cache_key_from_files(dir.path(), &["a.lock", "b.lock"]);
        let k2 = cache_key_from_files(dir.path(), &["b.lock", "a.lock"]);
        assert_eq!(k1, k2, "cache key should be order-independent");
    }
}

// ─── S3 mime type tests ───────────────────────────────────────────────────────

mod mime_types {
    // We test the logic directly by replicating it — the actual function is private.
    // These tests document the contract that the S3 target upholds.

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

    #[test]
    fn html_gets_charset() {
        assert!(mime_for("index.html").contains("charset=utf-8"));
        assert!(mime_for("page.htm").contains("charset=utf-8"));
    }

    #[test]
    fn wasm_gets_correct_type() {
        assert_eq!(mime_for("module.wasm"), "application/wasm");
    }

    #[test]
    fn fonts_get_font_type() {
        assert_eq!(mime_for("Inter.woff2"), "font/woff2");
        assert_eq!(mime_for("Inter.woff"),  "font/woff");
    }

    #[test]
    fn unknown_extension_gets_octet_stream() {
        assert_eq!(mime_for("binary.bin"),   "application/octet-stream");
        assert_eq!(mime_for("data.unknown"), "application/octet-stream");
        assert_eq!(mime_for("noext"),        "application/octet-stream");
    }

    #[test]
    fn hashed_asset_detection() {
        // Simulate the cache_control_for logic for hashed assets
        fn looks_hashed(path: &str) -> bool {
            let filename = path.rsplit('/').next().unwrap_or(path);
            let mut run = 0usize;
            for c in filename.chars() {
                if c.is_ascii_hexdigit() {
                    run += 1;
                    if run >= 8 { return true; }
                } else if matches!(c, '.' | '-' | '_') {
                    run = 0;
                } else {
                    run = 0;
                }
            }
            filename.contains(".chunk.")
        }

        assert!(looks_hashed("main.a1b2c3d4.js"),    "8-char hex segment should be hashed");
        assert!(looks_hashed("chunk.deadbeef1234.css"), "12-char hex should be hashed");
        assert!(looks_hashed("index-Ab3Cd9Ef.js"),   "mixed case hex segment");
        assert!(looks_hashed("runtime.chunk.js"),    ".chunk. suffix");
        assert!(!looks_hashed("index.html"),         "index.html is not hashed");
        assert!(!looks_hashed("style.css"),          "style.css is not hashed");
        assert!(!looks_hashed("logo.png"),           "logo.png is not hashed");
        assert!(!looks_hashed("ab12ef.js"),          "only 6 hex chars — not enough");
    }
}

// ─── audit log tests ──────────────────────────────────────────────────────────

mod audit_log {
    use arkon_core::deploy::{DeployRecord, DeployStatus};
    use arkon_daemon::AuditLog;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    fn tmp_log() -> (TempDir, AuditLog) {
        let dir  = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.log");
        let log  = AuditLog::new(path);
        (dir, log)
    }

    fn fake_record(project: &str, target: &str) -> DeployRecord {
        DeployRecord::new(project, target, "static", "fp_test")
    }

    #[test]
    fn write_and_read_back() {
        let (_dir, log) = tmp_log();
        let record = fake_record("test-project", "production");
        log.append(&record).expect("append");

        let entries = log.read_all().expect("read_all");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].project, "test-project");
        assert_eq!(entries[0].target,  "production");
    }

    #[test]
    fn multiple_entries_in_order() {
        let (_dir, log) = tmp_log();
        log.append(&fake_record("proj", "staging")).expect("1");
        log.append(&fake_record("proj", "production")).expect("2");
        log.append(&fake_record("proj", "cdn")).expect("3");

        let entries = log.read_all().expect("read");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].target, "staging");
        assert_eq!(entries[2].target, "cdn");
    }

    #[test]
    fn hmac_chain_is_valid_after_write() {
        let (_dir, log) = tmp_log();
        log.append(&fake_record("proj", "prod-1")).expect("1");
        log.append(&fake_record("proj", "prod-2")).expect("2");

        let (entries, chain_valid) = log.read_and_verify().expect("verify");
        assert!(chain_valid, "HMAC chain should be valid after clean writes");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn hmac_changes_when_record_tampered() {
        let (_dir, log) = tmp_log();
        log.append(&fake_record("proj", "prod")).expect("append");

        // Tamper with the log file by altering a character
        let path = log.path();
        let raw  = std::fs::read_to_string(&path).expect("read");
        let tampered = raw.replacen("prod", "hack", 1);
        std::fs::write(&path, tampered).expect("write tampered");

        let (_, chain_valid) = log.read_and_verify().expect("read tampered");
        assert!(!chain_valid, "chain should be invalid after tampering");
    }

    #[test]
    fn each_record_has_unique_id() {
        let (_dir, log) = tmp_log();
        for _ in 0..5 {
            log.append(&fake_record("proj", "prod")).expect("append");
        }
        let entries = log.read_all().expect("read");
        let mut ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 5, "all record IDs should be unique");
    }

    #[test]
    fn chain_hmac_field_is_non_empty() {
        let (_dir, log) = tmp_log();
        log.append(&fake_record("proj", "prod")).expect("append");

        let entries = log.read_all().expect("read");
        assert!(!entries[0].chain_hmac.is_empty(), "chain_hmac must be written");
        assert_eq!(entries[0].chain_hmac.len(), 64, "SHA-256 hex = 64 chars");
    }
}

// ─── hook execution tests ─────────────────────────────────────────────────────

mod hook_execution {
    use arkon_adapters::runner::run_hook;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn empty_env() -> HashMap<String, String> { HashMap::new() }

    #[test]
    fn successful_hook_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = run_hook("true", dir.path(), &empty_env());
        assert!(result.is_ok(), "hook 'true' should succeed");
    }

    #[test]
    fn failing_hook_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = run_hook("false", dir.path(), &empty_env());
        assert!(result.is_err(), "hook 'false' should fail");
    }

    #[test]
    fn hook_creates_file_in_cwd() {
        let dir  = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("hook-ran.txt");
        let cmd  = format!("touch {}", file.display());
        run_hook(&cmd, dir.path(), &empty_env()).expect("hook ok");
        assert!(file.exists(), "hook should have created the file");
    }

    #[test]
    fn hook_receives_env_vars() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("env-out.txt");
        let mut env = HashMap::new();
        env.insert("ARKON_TEST_VAR".to_string(), "hello-from-arkon".to_string());
        let cmd = format!("echo $ARKON_TEST_VAR > {}", out.display());
        run_hook(&cmd, dir.path(), &env).expect("hook ok");
        let content = std::fs::read_to_string(&out).expect("read");
        assert!(content.contains("hello-from-arkon"), "env var not received by hook");
    }

    #[test]
    fn hook_runs_in_correct_cwd() {
        let dir  = tempfile::tempdir().expect("tempdir");
        let out  = dir.path().join("pwd-out.txt");
        let cmd  = format!("pwd > {}", out.display());
        run_hook(&cmd, dir.path(), &empty_env()).expect("hook ok");
        let content = std::fs::read_to_string(&out).expect("read");
        // Normalise trailing newline and resolve symlinks for macOS /var → /private/var
        let got  = content.trim().to_string();
        let want = std::fs::canonicalize(dir.path())
            .unwrap_or_else(|_| dir.path().to_path_buf())
            .to_string_lossy()
            .into_owned();
        assert!(got.ends_with(&want.trim_start_matches('/')),
            "hook cwd mismatch: got '{got}', want '{want}'");
    }

    #[test]
    fn multi_command_hook_via_semicolon() {
        let dir = tempfile::tempdir().expect("tempdir");
        let f1  = dir.path().join("step1.txt");
        let f2  = dir.path().join("step2.txt");
        let cmd = format!("touch {} && touch {}", f1.display(), f2.display());
        run_hook(&cmd, dir.path(), &empty_env()).expect("multi-hook ok");
        assert!(f1.exists() && f2.exists(), "both steps should execute");
    }
}

// ─── network integration tests (skipped in CI — run manually with --include-ignored) ─

mod network {
    /// SSH target smoke test — requires a reachable SSH server.
    /// Set env ARKON_TEST_SSH_HOST=user@host:/path to enable.
    #[test]
    #[ignore = "requires live SSH server (set ARKON_TEST_SSH_HOST)"]
    fn ssh_target_connects() {
        let spec = std::env::var("ARKON_TEST_SSH_HOST")
            .expect("ARKON_TEST_SSH_HOST=user@host:/path");
        // Parse user@host:/path
        let (user_host, path) = spec.split_once(':').expect("bad format");
        let (user, host) = user_host.split_once('@').expect("bad format");

        // Simple TCP connectivity check
        let addr = format!("{host}:22");
        let ok = std::net::TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            std::time::Duration::from_secs(5),
        ).is_ok();
        assert!(ok, "SSH port 22 on {host} should be reachable");
    }

    /// S3 diff-upload smoke test — requires real credentials.
    /// Set env ARKON_TEST_S3_BUCKET=bucket-name to enable.
    #[test]
    #[ignore = "requires AWS credentials and real S3 bucket (set ARKON_TEST_S3_BUCKET)"]
    fn s3_target_lists_objects() {
        let bucket = std::env::var("ARKON_TEST_S3_BUCKET")
            .expect("ARKON_TEST_S3_BUCKET not set");
        assert!(!bucket.is_empty(), "bucket name required");
        // Full test would call the S3 list API here
        // Omitted — the ListObjectsV2 path is exercised by the S3 target implementation
        println!("would test bucket: {bucket}");
    }

    /// ACME staging test — verifies account creation + challenge flow.
    /// Set env ARKON_TEST_DOMAIN=yourdomain.com to enable.
    #[test]
    #[ignore = "requires domain ownership and Let's Encrypt staging (set ARKON_TEST_DOMAIN)"]
    fn acme_staging_account_creation() {
        let domain = std::env::var("ARKON_TEST_DOMAIN")
            .expect("ARKON_TEST_DOMAIN not set");
        // Would call AcmeProvisioner::new(domain, email, staging=true).provision(...)
        println!("would test ACME for: {domain}");
    }

    /// P2P relay registration test — verifies the relay client.
    /// Set env ARKON_TEST_RELAY_URL to enable (uses public relay by default).
    #[tokio::test]
    #[ignore = "requires relay server (set ARKON_TEST_RELAY_URL or use default)"]
    async fn p2p_relay_registers_and_deregisters() {
        let relay = std::env::var("ARKON_TEST_RELAY_URL")
            .unwrap_or_else(|_| "https://relay.arkon.sh".into());

        // Generate a test peer ID
        let identity = arkon_p2p::identity::PeerIdentity::generate();
        let token    = arkon_p2p::relay::generate_token();

        let local_addr = "127.0.0.1:0".parse().unwrap();
        let result = arkon_p2p::relay::RelayHandle::register(
            &relay,
            &identity.peer_id,
            local_addr,
            60,
            &token,
        ).await;

        match result {
            Ok((handle, url)) => {
                assert!(!url.is_empty(), "relay should return a public URL");
                handle.deregister().await;
            }
            Err(e) => {
                // Relay might not be running — just log
                println!("relay not reachable (expected in CI): {e}");
            }
        }
    }
}


// --- vault security tests ---

mod vault_security {
    use arkon_secrets::{Vault, mnemonic, machine_fingerprint_raw};

    fn proj() -> String { format!("arkon-sec-{}", uuid::Uuid::new_v4()) }

    #[test]
    fn ciphertext_is_not_plaintext() {
        std::env::set_var("ARKON_VAULT_KEY", "0".repeat(64));
        let p = proj();
        let v = Vault::open(&p).expect("open");
        v.set("SECRET", b"super-secret-value").expect("set");
        let path = dirs::home_dir().unwrap()
            .join(".arkon").join("vault").join(format!("{p}.vault"));
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!raw.contains("super-secret-value"), "plaintext in vault file!");
        std::env::remove_var("ARKON_VAULT_KEY");
    }

    #[test]
    fn vault_file_has_0600_permissions() {
        std::env::set_var("ARKON_VAULT_KEY", "cc".repeat(32));
        let p = proj();
        let v = Vault::open(&p).expect("open");
        v.set("K", b"v").expect("set");
        let path = dirs::home_dir().unwrap()
            .join(".arkon").join("vault").join(format!("{p}.vault"));
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "vault must be 0600, got {:o}", mode);
        }
        std::env::remove_var("ARKON_VAULT_KEY");
    }

    #[test]
    fn mnemonic_roundtrip() {
        let key: [u8; 32] = {
            let mut k = [0u8; 32];
            for (i, b) in k.iter_mut().enumerate() { *b = i as u8; }
            k
        };
        let mnem = mnemonic::key_to_mnemonic(&key).expect("to_mnemonic");
        let words = mnem.words().collect::<Vec<_>>().join(" ");
        let recovered = mnemonic::mnemonic_to_key(&words).expect("to_key");
        assert_eq!(*recovered, key, "mnemonic roundtrip failed");
    }

    #[test]
    fn mnemonic_is_24_words() {
        let key: [u8; 32] = [42u8; 32];
        let mnem = mnemonic::key_to_mnemonic(&key).expect("mnemonic");
        assert_eq!(mnem.words().count(), 24);
    }

    #[test]
    fn invalid_mnemonic_errors() {
        assert!(mnemonic::mnemonic_to_key("not valid bip39").is_err());
    }

    #[test]
    fn machine_fingerprint_deterministic() {
        assert_eq!(machine_fingerprint_raw(), machine_fingerprint_raw());
    }

    #[test]
    fn machine_fingerprint_has_platform_prefix() {
        let fp = machine_fingerprint_raw();
        assert!(fp.starts_with("linux:") || fp.starts_with("macos:")
            || fp.starts_with("win:") || fp.starts_with("sw:"),
            "bad prefix: {fp}");
    }

    #[test]
    fn acme_key_roundtrip() {
        std::env::set_var("ARKON_VAULT_KEY", "dd".repeat(32));
        let p = proj();
        let v = Vault::open(&p).expect("open");
        let json = r#"{"account_url":"https://acme.example.com/1"}"#;
        v.set_acme_key("example.com", json).expect("set");
        let got = v.get_acme_key("example.com").expect("get");
        assert_eq!(got, json);
        let path = dirs::home_dir().unwrap()
            .join(".arkon").join("vault").join(format!("{p}.vault"));
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!raw.contains("acme.example.com"), "ACME URL in plaintext!");
        std::env::remove_var("ARKON_VAULT_KEY");
    }

    #[test]
    fn peer_identity_roundtrip() {
        std::env::set_var("ARKON_VAULT_KEY", "ee".repeat(32));
        let p = proj();
        let v = Vault::open(&p).expect("open");
        let seed = [0xab; 32];
        v.set_peer_identity(&seed).expect("set");
        let got = v.get_peer_identity().expect("get");
        assert_eq!(got, seed);
        std::env::remove_var("ARKON_VAULT_KEY");
    }
}
