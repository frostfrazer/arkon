use std::path::Path;

/// A single detection rule. Each adapter registers one or more.
pub struct DetectionRule {
    /// The adapter this rule fires for.
    pub adapter: &'static str,
    /// Human-readable description shown in `arkon detect --verbose`.
    pub description: &'static str,
    /// Confidence score if ALL required files are present. 0.0–1.0.
    pub confidence: f32,
    /// Files/dirs that MUST exist (all of them).
    pub required: &'static [&'static str],
    /// Files/dirs that MUST NOT exist (disqualifiers).
    pub excluded: &'static [&'static str],
    /// Optional bonus files: each present one adds `bonus` to confidence.
    pub bonus: &'static [(&'static str, f32)],
}

impl DetectionRule {
    pub fn score(&self, root: &Path) -> f32 {
        // All required files must exist
        let all_required = self.required.iter().all(|f| root.join(f).exists());
        if !all_required {
            return 0.0;
        }
        // Any excluded file → disqualify
        let any_excluded = self.excluded.iter().any(|f| root.join(f).exists());
        if any_excluded {
            return 0.0;
        }
        // Base confidence + bonuses
        let bonus: f32 = self
            .bonus
            .iter()
            .filter(|(f, _)| root.join(f).exists())
            .map(|(_, b)| b)
            .sum();
        (self.confidence + bonus).min(1.0)
    }
}

/// All built-in detection rules, evaluated in order.
/// Highest scoring rule ≥ 0.6 wins.
pub static RULES: &[DetectionRule] = &[
    // ── Game engines (highest specificity first) ──────────────────────────
    DetectionRule {
        adapter: "unity",
        description: "Unity game project",
        confidence: 0.99,
        required: &["Assets", "ProjectSettings"],
        excluded: &[],
        bonus: &[
            ("Packages/manifest.json", 0.005),
            ("UnityLockfile",          0.005),
        ],
    },
    DetectionRule {
        adapter: "godot",
        description: "Godot game project",
        confidence: 0.99,
        required: &["project.godot"],
        excluded: &[],
        bonus: &[("export_presets.cfg", 0.005)],
    },
    DetectionRule {
        adapter: "bevy",
        description: "Bevy game (Rust)",
        confidence: 0.92,
        required: &["Cargo.toml", "src/main.rs"],
        excluded: &[],
        bonus: &[("assets/", 0.05)], // Bevy projects almost always have assets/
    },

    // ── Web frameworks ────────────────────────────────────────────────────
    DetectionRule {
        adapter: "nextjs",
        description: "Next.js application",
        confidence: 0.98,
        required: &["package.json"],
        excluded: &[],
        bonus: &[
            ("next.config.js",  0.30),
            ("next.config.ts",  0.30),
            ("next.config.mjs", 0.28),
            (".next/",          0.10),
        ],
    },
    DetectionRule {
        adapter: "astro",
        description: "Astro site",
        confidence: 0.97,
        required: &["package.json"],
        excluded: &[],
        bonus: &[
            ("astro.config.mjs", 0.40),
            ("astro.config.ts",  0.40),
            ("src/pages/",       0.10),
        ],
    },
    DetectionRule {
        adapter: "vite",
        description: "Vite project (React / Vue / Svelte / Solid)",
        confidence: 0.95,
        required: &["package.json"],
        excluded: &["next.config.js", "next.config.ts", "astro.config.mjs"],
        bonus: &[
            ("vite.config.js",  0.35),
            ("vite.config.ts",  0.35),
            ("vite.config.mjs", 0.33),
        ],
    },
    DetectionRule {
        adapter: "sveltekit",
        description: "SvelteKit application",
        confidence: 0.97,
        required: &["package.json", "svelte.config.js"],
        excluded: &[],
        bonus: &[("src/routes/", 0.10)],
    },
    DetectionRule {
        adapter: "nuxt",
        description: "Nuxt application",
        confidence: 0.97,
        required: &["package.json"],
        excluded: &[],
        bonus: &[
            ("nuxt.config.ts", 0.40),
            ("nuxt.config.js", 0.40),
        ],
    },

    // ── Static site generators ────────────────────────────────────────────
    DetectionRule {
        adapter: "hugo",
        description: "Hugo static site",
        confidence: 0.96,
        required: &["hugo.toml"],
        excluded: &[],
        bonus: &[("content/", 0.05), ("layouts/", 0.05)],
    },
    DetectionRule {
        adapter: "jekyll",
        description: "Jekyll static site",
        confidence: 0.95,
        required: &["_config.yml"],
        excluded: &[],
        bonus: &[("Gemfile", 0.05), ("_posts/", 0.05)],
    },
    DetectionRule {
        adapter: "eleventy",
        description: "Eleventy static site",
        confidence: 0.94,
        required: &["package.json", ".eleventy.js"],
        excluded: &[],
        bonus: &[],
    },

    // ── Backend runtimes ──────────────────────────────────────────────────
    DetectionRule {
        adapter: "nodejs",
        description: "Node.js server (Express / Fastify / Hono)",
        confidence: 0.80,
        required: &["package.json"],
        excluded: &[
            "next.config.js", "next.config.ts",
            "vite.config.js", "vite.config.ts",
            "astro.config.mjs", "svelte.config.js",
            "nuxt.config.ts",
        ],
        bonus: &[("server.js", 0.10), ("src/index.js", 0.08), ("src/index.ts", 0.08)],
    },
    DetectionRule {
        adapter: "python",
        description: "Python application (Flask / FastAPI / Django)",
        confidence: 0.85,
        required: &["requirements.txt"],
        excluded: &[],
        bonus: &[
            ("pyproject.toml",  0.10),
            ("manage.py",       0.12), // Django
            ("app.py",          0.08),
            ("main.py",         0.06),
        ],
    },
    DetectionRule {
        adapter: "python",
        description: "Python application (pyproject.toml)",
        confidence: 0.83,
        required: &["pyproject.toml"],
        excluded: &[],
        bonus: &[("src/", 0.05)],
    },
    DetectionRule {
        adapter: "go",
        description: "Go application",
        confidence: 0.93,
        required: &["go.mod"],
        excluded: &[],
        bonus: &[("main.go", 0.10), ("cmd/", 0.08)],
    },
    DetectionRule {
        adapter: "rust-bin",
        description: "Rust binary (non-game)",
        confidence: 0.87,
        required: &["Cargo.toml", "src/main.rs"],
        excluded: &["project.godot", "Assets"], // exclude Bevy/game projects
        bonus: &[],
    },
    DetectionRule {
        adapter: "deno",
        description: "Deno application",
        confidence: 0.92,
        required: &["deno.json"],
        excluded: &[],
        bonus: &[("deno.lock", 0.05)],
    },
    DetectionRule {
        adapter: "bun",
        description: "Bun application",
        confidence: 0.91,
        required: &["package.json", "bun.lockb"],
        excluded: &[],
        bonus: &[],
    },

    // ── Container ─────────────────────────────────────────────────────────
    DetectionRule {
        adapter: "docker",
        description: "Dockerfile-based container",
        confidence: 0.90,
        required: &["Dockerfile"],
        excluded: &[],
        bonus: &[("docker-compose.yml", 0.05), (".dockerignore", 0.03)],
    },

    // ── Generic shell build ───────────────────────────────────────────────
    DetectionRule {
        adapter: "shell",
        description: "Generic shell build script",
        confidence: 0.60,
        required: &["build.sh"],
        excluded: &[],
        bonus: &[],
    },

    // ── Plain static (lowest priority) ────────────────────────────────────
    DetectionRule {
        adapter: "static",
        description: "Plain static HTML site",
        confidence: 0.70,
        required: &["index.html"],
        excluded: &[
            "package.json", "Cargo.toml", "go.mod",
            "requirements.txt", "pyproject.toml",
        ],
        bonus: &[
            ("style.css",  0.05),
            ("script.js",  0.05),
            ("404.html",   0.05),
        ],
    },

    // ── Mobile ────────────────────────────────────────────────────────────
    DetectionRule {
        adapter: "android",
        description: "Android application (Gradle)",
        confidence: 0.96,
        required: &["gradlew", "app/build.gradle"],
        excluded: &[],
        bonus: &[
            ("app/build.gradle.kts",   0.05),
            ("app/src/main/AndroidManifest.xml", 0.10),
        ],
    },
    DetectionRule {
        adapter: "android",
        description: "Android application (Gradle kts)",
        confidence: 0.95,
        required: &["gradlew", "app/build.gradle.kts"],
        excluded: &[],
        bonus: &[
            ("app/src/main/AndroidManifest.xml", 0.10),
        ],
    },
    DetectionRule {
        adapter: "ios",
        description: "iOS application (Xcode)",
        confidence: 0.97,
        required: &["*.xcodeproj"],   // checked via bonus since glob not supported in required
        excluded: &[],
        bonus: &[
            ("Podfile",          0.30),
            ("Package.swift",    0.20),
            ("Podfile.lock",     0.10),
        ],
    },
];
