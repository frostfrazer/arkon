# ARKON
### Automated Runtime & Kernel Orchestration Node

> A single Rust binary that detects any project, builds it locally, and ships it to infrastructure you control. Zero mandatory platform fees. One command.

[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)
[![rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://rustup.rs)

---

## Why ARKON?

Every deployment platform today inserts itself as a middleman — charging per bandwidth, per seat, per build minute. ARKON removes the middleman entirely. Your machine builds the artifact. Your SSH key pushes it. Your S3 bucket stores it. ARKON is the client agent that orchestrates all of it, locally, with no recurring fees beyond what you already pay for your own hardware.

---

## Install

```bash
# One-line installer (Linux / macOS)
curl -fsSL https://arkon.sh/install.sh | sh

# Or build from source
cargo install --git https://github.com/arkon-sh/arkon arkon-cli
```

**Requirements:** Rust 1.75+, git, rsync (SSH target), Docker (optional), IPFS daemon (optional).

---

## Quick start

```bash
arkon init                     # generate arkon.toml
arkon secrets set DB_URL       # store secrets in AES-256 vault
arkon                          # detect → build → deploy interactively
```

---

## Commands

| Command | Description |
|---------|-------------|
| `arkon` | Interactive detect → build → deploy |
| `arkon ship` | Non-interactive deploy |
| `arkon ship --target staging` | Deploy to specific target |
| `arkon ship --json` | Machine-readable output for CI/CD |
| `arkon detect [--verbose]` | Show adapter + confidence |
| `arkon preview [--ttl 4h]` | P2P WebRTC preview tunnel |
| `arkon rollback` | Roll back to latest snapshot |
| `arkon rollback --to 2024-11-03` | Roll back by date |
| `arkon rollback --to a1b2c3d4` | Roll back by snapshot ID |
| `arkon rollback list` | List available snapshots |
| `arkon promote staging production` | Promote without rebuild |
| `arkon status` | Health check all targets |
| `arkon log [--limit 10]` | Deploy history + HMAC verification |
| `arkon cost` | Estimate deploy cost |
| `arkon doctor` | Check system dependencies |
| `arkon init` | Generate arkon.toml |
| `arkon secrets set KEY` | Store secret in vault |
| `arkon secrets list` | List secret keys |
| `arkon secrets export` | Export vault as .env |
| `arkon adapter list` | List adapters |
| `arkon adapter add <git-url>` | Install community adapter |
| `arkon adapter reload` | Hot-reload adapters |
| `arkon serve` | Run background daemon |

All commands accept `--json` (machine-readable) and `--root <path>`.

---

## arkon.toml

```toml
[project]
name    = "my-app"
adapter = "nextjs"       # auto-detected if omitted
cache   = true

[build]
command    = "npm run build"
output_dir = "dist"

[deploy]
default_target     = "production"
confirm_cost       = true
cost_threshold_usd = 0.10

[targets.production]
type   = "ssh"
host   = "myserver.com"
user   = "deploy"
path   = "/var/www/app"
tls    = true
domain = "myapp.com"

[targets.staging]
type       = "s3"
bucket     = "my-app-staging"
region     = "eu-central-1"
invalidate = true

[targets.game_dl]
type    = "b2"
bucket  = "my-game-builds"
torrent = true
dl_page = true

[targets.preview]
type = "webrtc"
ttl  = "24h"

[targets.pages]
type   = "github-pages"
repo   = "owner/my-app"
branch = "gh-pages"

[targets.ipfs]
type        = "ipfs"
pin_service = "https://api.web3.storage"

[hooks.production]
pre_deploy  = ["npm run db:migrate"]
post_deploy = ["pm2 restart app"]

[health.production]
checks   = [
  { type = "http", url = "https://myapp.com/healthz", expect = 200 },
  { type = "tcp",  host = "myapp.com", port = 5432 },
]
interval   = "60s"
retries    = 3
on_failure = "https://hooks.slack.com/..."

[adapters]
sources    = ["https://github.com/my-org/arkon-django-adapter"]
hot_reload = true
```

---

## Secrets vault

```bash
arkon secrets set DATABASE_URL
arkon secrets set AWS_ACCESS_KEY_ID
arkon secrets set GITHUB_TOKEN

# Secrets are AES-256-GCM encrypted at ~/.arkon/vault/<project>.vault
# Injected as env vars at build time and in hooks
# Never written to arkon.toml
```

---

## Deployment targets

| Target | Type key | Auth source | Notes |
|--------|----------|-------------|-------|
| SSH / VPS | `ssh` | `~/.ssh` | rsync, auto-TLS, pm2 hooks |
| AWS S3 | `s3` | vault | diff-aware, CloudFront invalidation |
| Cloudflare R2 | `r2` | vault | diff-aware, free egress |
| Backblaze B2 | `b2` | vault | cheapest storage, torrent seeder |
| GitHub Pages | `github-pages` | `GITHUB_TOKEN` vault | shallow clone + force-push |
| IPFS | `ipfs` | `IPFS_PIN_TOKEN` vault | Kubo RPC + remote pin |
| P2P WebRTC | `webrtc` | none | zero-cost, no open ports |
| Docker registry | `docker` | vault | local bollard build + push |
| Local export | `local` | — | copy to local directory |

---

## Supported adapters

**Web:** Next.js · Vite · Astro · SvelteKit · Nuxt · Hugo · Jekyll · Eleventy · Static HTML

**Backend:** Node.js · Python (Flask/FastAPI/Django) · Go · Rust · Deno · Bun · Docker

**Games:** Unity (Win/Linux/WebGL) · Godot · Bevy · Pygame · LÖVE 2D · Electron · Tauri

**Generic:** Shell (build.sh)

```bash
# Community adapters — no marketplace, no approval
arkon adapter add https://github.com/my-org/arkon-laravel-adapter
```

---

## CI/CD

```yaml
# GitHub Actions
- name: Deploy to production
  run: arkon ship --target production --json | tee deploy.json
  env:
    ARKON_VAULT_KEY: ${{ secrets.ARKON_VAULT_KEY }}

- name: Verify deploy
  run: jq -e '.ok == true' deploy.json
```

**JSON output schema (stable across minor versions):**
```json
{
  "ok": true,
  "version": "0.1.0",
  "command": "ship",
  "project": "my-app",
  "adapter": "nextjs",
  "target": "production",
  "url": "https://myapp.com",
  "snapshot_id": "a1b2c3d4",
  "artifact_fingerprint": "deadbeef1234abcd",
  "size_bytes": 4194304,
  "duration_ms": 12430,
  "deployed_at": "2024-11-03T14:22:00Z"
}
```

---

## Architecture

```
arkon
├── Project Detector    confidence-scored adapter selection (18 rules, 0.0–1.0)
├── Adapter Registry    10 built-in + community Git adapters, hot-reload
│   └── Build Runner   cache key → pre_build → build → fingerprint → post_build
├── Dispatcher         hooks → cost estimate → target → snapshot → audit log
│   ├── SSH            rsync + ACME TLS + pm2
│   ├── S3/R2/B2       diff-aware upload + CDN invalidation
│   ├── GitHub Pages   git clone/wipe/copy/force-push
│   ├── IPFS           Kubo HTTP RPC + pinning services API
│   ├── Docker         bollard local build + registry push
│   ├── WebRTC         arkon-p2p Ed25519 peer + relay + local HTTP server
│   └── Torrent        lava_torrent + tracker announce + HTML download page
├── Snapshot Store     ~/.arkon/snapshots/ — rollback + promote + prune
├── Secrets Vault      AES-256-GCM + Argon2id KDF at ~/.arkon/vault/
├── Audit Log          HMAC-chained NDJSON at ~/.arkon/audit.log
└── Daemon             health monitor + adapter watcher + ACME renewer + pruner
```

---

## Security

- SSH keys are user-managed (`~/.ssh`)
- Secrets: AES-256-GCM, Argon2id KDF, machine-locked
- P2P: real Ed25519 keypairs per project
- Audit log: HMAC chain verified on every `arkon log`
- No data leaves your network without explicit target configuration
- No privilege escalation — runs entirely as your user

---

## Workspace

```
crates/
  arkon-core       Artifact, Config, DeployRecord, Snapshot, Error
  arkon-detector   18-rule confidence scorer
  arkon-adapters   Adapter trait + 10 built-ins + community registry
  arkon-targets    Target trait + 8 built-ins + Dispatcher
  arkon-secrets    AES-256-GCM vault
  arkon-store      Snapshot store: save/find/list/prune/rollback
  arkon-p2p        Ed25519 identity, local HTTP server, relay client
  arkon-daemon     Health monitor, audit log, watcher, ACME, pruner
  arkon-cli        Full CLI, JSON output, build.rs version embedding
  arkon-tests      Integration test suite
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

MIT OR Apache-2.0
