# Contributing to ARKON

Thank you for improving ARKON. This document covers:

1. [Reporting bugs](#reporting-bugs)
2. [Submitting pull requests](#submitting-pull-requests)
3. [Writing a community adapter](#writing-a-community-adapter)
4. [Writing a community target](#writing-a-community-target)

---

## Reporting bugs

Open a GitHub issue with:
- ARKON version (`arkon --version`)
- OS + architecture
- `arkon doctor` output
- Minimal reproduction steps
- Full error output (run with `-vvv` for trace logging)

---

## Submitting pull requests

```bash
# Fork and clone
git clone https://github.com/your-fork/arkon
cd arkon

# Build
cargo build

# Run tests
cargo test -p arkon-tests

# Lint
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

All PRs require:
- Passing CI (Linux + macOS + Windows)
- New or updated tests in `arkon-tests`
- Entry in `CHANGELOG.md` under `[Unreleased]`
- No new `unwrap()` in library code — use `?` and `ArkonError`

---

## Writing a community adapter

A community adapter is a Git repository containing one or more JSON manifest files.
ARKON loads them with `arkon adapter add <git-url>` and hot-reloads them at runtime.

### JSON manifest format

Create `my-adapter.json` in your repo root:

```json
{
  "name": "laravel",
  "description": "Laravel PHP application",
  "build_command": "composer install --no-dev && php artisan config:cache",
  "output_dir": ".",
  "deployable_type": "container",
  "cache_inputs": ["composer.lock", "composer.json", "artisan"]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | ✅ | Adapter identifier — must be unique |
| `description` | string | ✅ | Shown in `arkon detect --verbose` |
| `build_command` | string | ✅ | Shell command to build the project |
| `output_dir` | string | ✅ | Where artifacts land (relative to project root) |
| `deployable_type` | `"static"` \| `"container"` \| `"game_build"` \| `"wasm"` \| `"binary"` | ✅ | Artifact kind |
| `cache_inputs` | array of strings | ✅ | Files fingerprinted for build cache key |

### Detection

JSON manifest adapters are loaded into the registry but do not auto-detect.
To enable auto-detection, implement the full `Adapter` trait in Rust (see below).

### Full Rust adapter

For detection + advanced logic, implement the `Adapter` trait in your crate and publish it as a Cargo dependency. Add a `[package.metadata.arkon]` section so ARKON can discover it:

```toml
# In your adapter crate's Cargo.toml
[package.metadata.arkon]
adapter = "my-adapter"
```

```rust
use arkon_adapters::{Adapter, BuildCtx};
use arkon_core::{artifact::{Artifact, DeployableKind}, error::Result, runtime::Runtime};

pub struct MyAdapter;

impl Adapter for MyAdapter {
    fn name(&self) -> &str { "my-adapter" }
    fn description(&self) -> &str { "My custom framework" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        arkon_adapters::runner::run_shell_command("my-build-command", &ctx.root, &ctx.env)?;
        let output = self.output_dir(ctx);
        let (fp, hashes, size) = arkon_adapters::fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("my-adapter", output, DeployableKind::Static);
        art.fingerprint  = fp;
        art.file_hashes  = hashes;
        art.size_bytes   = size;
        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> std::path::PathBuf {
        ctx.root.join("build")
    }

    fn runtime_info(&self) -> Runtime { Runtime::static_files() }
    fn deployable_type(&self) -> DeployableKind { DeployableKind::Static }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        arkon_adapters::fingerprint::cache_key_from_files(&ctx.root, &["composer.lock"])
    }
}
```

---

## Writing a community target

Targets follow the same pattern. Implement the `Target` trait:

```rust
use arkon_targets::Target;
use arkon_core::{artifact::Artifact, config::TargetConfig, deploy::DeployCtx, error::Result};

pub struct MyTarget;

impl Target for MyTarget {
    fn name(&self) -> &str { "my-target" }

    fn deploy(
        &self,
        artifact: &Artifact,
        ctx: &DeployCtx,
        config: &TargetConfig,
    ) -> Result<arkon_targets::DeployedUrl> {
        // your deploy logic
        todo!()
    }
}
```

Add a `[targets.my-target]` variant to `arkon-core`'s `TargetConfig` enum and open a PR.

---

## Code of conduct

Be kind. Assume good intent. Focus on the code, not the person.
