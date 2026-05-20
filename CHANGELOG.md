# Changelog

All notable changes to ARKON are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
ARKON follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Planned
- libp2p DHT bootstrap node operator guide
- P2P NAT traversal via STUN/TURN (hole-punching for symmetric NAT)
- App Store / Google Play distribution adapter

---

## [1.0.1] — Security hardening

### Critical fixes

- **Vault dual-factor encryption**: machine fingerprint is now a salt, not sole secret.
  `Argon2id(passphrase, SHA-256(machine_fingerprint || vault_salt))` — passphrase
  is the blind factor. Attacker with vault + machine-id still cannot decrypt.
- **BIP39 recovery mnemonic**: 24-word mnemonic on first vault creation.
  `arkon secrets recover` re-encrypts on new hardware. Hardware migration is now safe.
- **ACME key encrypted in vault**: was plaintext JSON. Now AES-256-GCM in vault.
- **P2P identity encrypted in vault**: was plaintext file (0600). Now in vault.
- **SSH strict host key verification**: default `StrictHostKeyChecking=yes`.
  `accept_new_host = true` in arkon.toml to opt in to accept-new.
- **OS keychain session cache**: passphrase cached via keyring crate.
  `arkon secrets lock` clears session.
- **Vault file permissions enforced**: 0700 dirs, 0600 vault files, atomic writes.
- **Relay clarified**: HTTP proxy (operator can read traffic). DHT direct = Noise encrypted.

### Added
- `arkon secrets recover` — BIP39 mnemonic vault recovery on new hardware
- `arkon secrets lock` — clear OS keychain session cache
- `arkon_secrets::keychain` — OS keychain integration (keyring crate)
- `arkon_secrets::mnemonic` — BIP39 generation and recovery
- `arkon_secrets::vault::Vault::open_with_key()` — raw key for recovery
- `accept_new_host` field on SSH target config
- 9 vault security integration tests

## [1.0.0] — Sprint 8 — v1.0.0

### Added
- **libp2p Kademlia DHT** (`arkon-p2p/src/dht/`) — true peer-to-peer preview without relay dependency
  - `DhtNode` — full libp2p swarm: Kademlia, Identify, TCP transport, Noise encryption, Yamux mux
  - `DhtProvider` — DHT-based preview session; announces under `SHA-256("arkon/preview/<name>")`
  - Re-announcement loop keeps DHT records alive for the full TTL
  - `ARKON_BOOTSTRAP_PEERS` env var for custom bootstrap node override
- **P2P game server matchmaking** (`arkon-p2p/src/dht/provider.rs`)
  - `GameServer::announce(game, version)` — registers server in DHT
  - `GameServer::find(game, version)` — discovers live servers, returns peer IDs
  - `InternetMatchmaker` — keepalive loop re-announces every 30 minutes
- **Offline WiFi LAN discovery** (`arkon-p2p/src/matchmaking/lan.rs`)
  - `LanBroadcaster` — announces `_arkon._tcp.local.` via mDNS/Bonjour/Avahi
  - `LanScanner` — discovers peers on the local network, no internet required
  - TXT records carry game name, version, peer ID, port, type
- **Android adapter** (`arkon-adapters/src/adapters/android.rs`)
  - Gradle build via `./gradlew assembleRelease`
  - Detects APK and AAB output, records primary artifact path
  - Validates `ANDROID_SDK_ROOT` and Java availability
  - Makes `gradlew` executable before build
- **iOS adapter** (`arkon-adapters/src/adapters/ios.rs`)
  - `xcodebuild archive` → `xcodebuild -exportArchive` → IPA
  - Auto-detects scheme from `.xcodeproj`
  - Generates `ExportOptions.plist` from vault `APPLE_TEAM_ID`
  - macOS-only guard with clear error message on other platforms
- **Detection rules** for Android and iOS added to the 18-rule detector
- **Self-hostable relay server** (`relay/main.go`) — zero-dependency Go binary
  - Peer registry with TTL-based expiry and 60s sweep loop
  - HTTP proxy from browser → local ARKON preview server
  - `/health` endpoint with peer count
  - Systemd service example in `relay/README.md`
- **LICENSE-MIT** and **LICENSE-APACHE** — dual-licensed MIT OR Apache-2.0
- **`.cargo/config.toml`** — cross-compilation linkers for musl, macOS, Windows; `git-fetch-with-cli`
- **Panicking calls eliminated** — `expect("failed to read password")` → `?`; `lock().unwrap()` → graceful poisoned-lock recovery

### Changed
- Workspace version bumped to **1.0.0**
- `AdapterRegistry::default()` now registers 12 built-in adapters (added android, ios)
- `adapter_registry` test updated to assert android and ios are present
- `arkon-p2p` exports `DhtNode`, `DhtProvider`, `LanBroadcaster`, `LanScanner`, `InternetMatchmaker`


## [0.7.0] — Sprint 7

### Added
- **GUI Deploy page** — full operational control panel: ship, rollback, and promote
  - Project file picker via `tauri-plugin-dialog`
  - Target selector populated from arkon.toml
  - Snapshot list for rollback with click-to-select
  - Promote from/to target picker
  - Dry-run checkbox
  - Real-time progress log streamed from Rust via `tauri::ipc::Channel<ProgressEvent>`
  - Result card with URL, snapshot ID, size, duration
- **GUI Cost page** — builds artifact and shows real cost estimate per target
- **GUI wired to Deploy page** — all 7 pages in sidebar nav; tray events route to Deploy
- **`tauri-plugin-dialog`** — project directory picker in Deploy and Cost pages
- **`AuditLog::new(path)`** — constructor for test-isolated log instances
- **`AuditLog::read_all()`** — read records without HMAC verification
- **`AuditLog::read_and_verify()`** — read + full HMAC chain verification, returns `(records, chain_valid)`
- **`AuditLog::path()`** — path accessor for test tampering
- **20 new tests** (total: 79):
  - Audit log: write/read-back, multiple entries, HMAC chain valid after write,
    HMAC broken after tamper, unique IDs, chain_hmac field length
  - Hook execution: success, failure, file creation, env var injection, cwd correctness, multi-command
  - Network `#[ignore]`: SSH connectivity, S3 listing, ACME staging, P2P relay

### Fixed
- `get_cost_estimate` IPC command now builds the real artifact and returns actual per-target estimates
- `get_status` IPC command handles all target types (S3 TCP probe, IPFS port check, GH Pages, WebRTC)
- GUI App.tsx registers tray events (`tray-ship`, `tray-preview`) and routes to Deploy page
- `AuditLog::open()` refactored to call `new_at()` shared implementation

## [0.5.0] — Sprint 5

### Added
- **Integration test suite** (`arkon-tests` crate) — 30+ tests covering:
  - Detector: all 10 built-in adapters, priority ordering, empty-dir failure
  - Fingerprint: stability, change detection, `FileDiff` add/modify/delete
  - Config: SSH/S3/health parse, missing file error, hooks
  - Snapshot store: save/find/list/delete/prune/reconstruct
  - JSON schema: success/error/detect/rollback shape validation
  - Adapter registry: all built-ins registered, unique names, unknown returns error
- **`git2` crate** replaces `std::process::Command` for adapter install
  - Real progress reporting (10% increments to stderr)
  - SSH agent credential integration
  - Shallow clone (depth=1)
- **Real hex-segment detection** in `cache_control_for` — replaces `"-abcdef"` string literal
  - Scans filename for runs of ≥8 hex chars separated by `.`/`-`/`_`
  - Sets `max-age=31536000, immutable` for hashed assets
- **CHANGELOG.md**, **CONTRIBUTING.md**, **install.sh** — project documentation complete
- **`.github/workflows/ci.yml`** — GitHub Actions: build + test matrix (Linux/macOS/Windows)
- **JSON error output** — when `--json` is set, errors emit `{"ok":false,"error":"..."}` to stderr
- **`arkon serve`** fully wired — starts health monitor, adapter watcher, and snapshot pruner as async tasks

### Fixed
- `adapter list` now emits JSON when `--json` is set
- `rollback`, `promote`, `preview` now emit `JsonResult` when `--json` is set
- S3 health probe in `status` uses TCP instead of `reqwest::blocking`
- SSH ACME integration wires `AcmeProvisioner` directly instead of shell-invoking `arkon-acme`
- Torrent seeder sends real HTTP tracker announce using `lava_torrent` info_hash

---

## [0.4.0] — Sprint 4

### Added
- **`arkon doctor`** — checks git, rsync, ssh, docker, IPFS daemon, SSH key, arkon.toml, vault
- **`arkon cost`** — estimates deploy cost per target without deploying; uses `Dispatcher::target_for()`
- **`arkon secrets export [--path file]`** — exports vault as `.env` with shell escaping and plaintext warning
- **`arkon init`** — generates complete `arkon.toml` template with all sections, auto-detected adapter hint
- **`--json`** wired into all commands — `ship`, `detect`, `rollback`, `promote`, `preview`, `status`, `log`, `adapter list`, `doctor`, `cost`
- **`build.rs`** — embeds `git rev-parse --short HEAD` and build date; banner shows `v0.4.0 (a1b2c3·2024-11-03)`
- **HMAC chain verification** in `arkon log` — full chain re-verified on every read; broken entries warned
- **Async target probes** in `arkon status` — per-type health checks with latency reporting
- **`Dispatcher::target_for()`** — read-only target accessor for cost estimation without deploy side-effects

### Changed
- `arkon log` now parses full `DeployRecord` structs instead of raw JSON lines
- `arkon status` S3 probe uses TCP+TLS port 443 instead of HTTP GET
- Error path in `main()` emits JSON when `--json` is active

---

## [0.3.0] — Sprint 3

### Added
- **Docker target** — `bollard` client builds image locally, auto-generates Dockerfile per artifact kind, pushes to any registry
- **IPFS target** — multipart upload to local Kubo node, local pin + remote pin via IPFS Pinning Services API spec
- **GitHub Pages target** — shallow clone, wipe, copy, `.nojekyll` + CNAME, force-push; `GITHUB_TOKEN` from vault
- **Torrent target** — `.torrent` via `lava_torrent`, tracker announce, minijinja HTML download page template
- **WebRTC target** — wraps `arkon-p2p` preview session as a first-class deploy target
- **Real Ed25519 keypairs** — `ed25519-dalek` replaces SHA-256 stub; peer IDs are cryptographically valid
- **`notify` crate** — inotify/kqueue FS events replace 10s poll loop; auto-fallback to poll on unsupported FS
- **ACME / Let's Encrypt** — full `instant-acme` HTTP-01 challenge flow, account caching, daily renewal scheduler
- **CloudFront invalidation** — real HTTP POST to CloudFront REST API
- **`--json` output mode** — `JsonResult` struct, `output_success`/`output_error`, tracing suppressed in JSON mode

### Changed
- `Dispatcher::default()` registers all 8 targets: ssh, s3, local, docker, ipfs, github-pages, torrent, webrtc

---

## [0.2.0] — Sprint 2

### Added
- **`arkon-store` crate** — snapshot persistence at `~/.arkon/snapshots/<project>/`
  - Timestamped JSON snapshots, NDJSON index, fuzzy lookup (ID prefix, date, "latest")
  - `prune(keep: N)` prevents unbounded disk growth
- **`arkon-p2p` crate** — P2P preview tunnel
  - Ed25519 peer identity per project at `~/.arkon/identity/<project>.key`
  - Async local HTTP static file server with SPA `index.html` fallback
  - Relay registration + keepalive heartbeat → `arkon preview` returns shareable link
- **`arkon rollback`** — fuzzy snapshot lookup, confirmation prompt, artifact reconstruction, re-dispatch, new snapshot saved
- **`arkon promote`** — loads latest snapshot for source target, re-dispatches to destination without rebuild
- **`arkon preview`** — builds artifact, registers with relay, returns live URL
- **`arkon adapter add <git-url>`** — clones repo, loads JSON manifests, hot-reloads
- **`AdapterWatcher`** — poll-based hot-reload (upgraded to notify in sprint 3)
- **`SnapshotPruner`** — daily async task, configurable keep count
- **S3 target** — full AWS SDK: `ListObjectsV2` diff → `PutObject` / batch `DeleteObjects`
  - Cloudflare cache purge via API (CF_API_TOKEN from vault)
  - Custom endpoint builder for B2, R2, MinIO

### Fixed
- Ship command saves snapshot after every successful deploy
- `arkon ship` result block shows snapshot ID

---

## [0.1.0] — Sprint 1 (MVP)

### Added
- **Rust workspace scaffold** — 7 crates with shared workspace deps
- **`arkon-core`** — `Artifact`, `ArkonConfig`, `DeployRecord`, `Snapshot`, `ArkonError`, `Runtime`, `CostHint`
- **`arkon-detector`** — 18-rule confidence scorer (0.0–1.0), `--verbose` candidate listing
- **`arkon-adapters`** — `Adapter` trait, `BuildCtx`, `BuildRunner` with cache key check
  - Built-in: `nextjs`, `vite`, `nodejs`, `python`, `go`, `rust-bin`, `docker`, `unity`, `static`, `shell`
  - `fingerprint_dir` — SHA-256 per-file + stable root hash, `FileDiff` compute
  - `AdapterRegistry` — community JSON manifest loader, hot-reload stub
- **`arkon-targets`** — `Target` trait, `Dispatcher` pipeline
  - SSH target: rsync over SSH, pre/post hooks, TLS provisioning stub
  - S3 target: stub (full SDK in sprint 2)
  - Local target: copy to directory
- **`arkon-secrets`** — AES-256-GCM vault with Argon2id KDF
- **`arkon-daemon`** — HMAC-chained audit log, async health monitor (HTTP/TCP/process)
- **`arkon-cli`** — full clap CLI: `ship`, `detect`, `init`, `secrets`, `status`, `log`, `adapter`, `serve`
- **`arkon.toml`** config with `[project]`, `[build]`, `[deploy]`, `[targets.*]`, `[hooks.*]`, `[health.*]`, `[adapters]`

---

## [0.0.0] — Sprint 0 (Scaffold)

### Added
- Workspace `Cargo.toml` with all shared dependencies pinned
- Core type stubs: `Adapter` trait, `Target` trait, `ArkonError`
- Project structure and crate layout established
