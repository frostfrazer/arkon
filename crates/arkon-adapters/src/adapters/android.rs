use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{
    artifact::{Artifact, DeployableKind},
    error::{ArkonError, Result},
    runtime::{Runtime, RuntimeKind},
};
use std::path::PathBuf;
use tracing::info;

pub struct AndroidAdapter;

impl Adapter for AndroidAdapter {
    fn name(&self) -> &str { "android" }
    fn description(&self) -> &str { "Android application (Gradle → APK/AAB)" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        // Verify Android SDK is available
        self.check_prerequisites(ctx)?;

        let cmd = ctx.command_override.as_deref()
            .unwrap_or("./gradlew assembleRelease");

        info!("building Android APK via Gradle");
        runner::run_shell_command(cmd, &ctx.root, &ctx.env)?;

        let output = self.output_dir(ctx);
        if !output.exists() {
            return Err(ArkonError::BuildFailed(format!(
                "Gradle output dir '{}' not found after build — check your Gradle config",
                output.display()
            )));
        }

        let (fp, hashes, size) = fingerprint::fingerprint_dir(&output)?;
        let mut art = Artifact::new("android", output, DeployableKind::Binary);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes  = size;

        // Find the primary APK/AAB and record it in metadata
        if let Some(apk) = self.find_primary_artifact(&art.path) {
            art = art.with_meta("primary_artifact", apk.to_string_lossy().as_ref());
        }

        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        // Standard Gradle output paths
        for candidate in &[
            "app/build/outputs/apk/release",
            "app/build/outputs/bundle/release",
            "build/outputs/apk/release",
        ] {
            let p = ctx.root.join(candidate);
            if p.exists() { return p; }
        }
        ctx.root.join("app/build/outputs/apk/release")
    }

    fn runtime_info(&self) -> Runtime {
        Runtime {
            kind:     RuntimeKind::Native,
            version:  None,
            env_keys: vec!["ANDROID_SDK_ROOT".into(), "JAVA_HOME".into()],
        }
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Binary }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        fingerprint::cache_key_from_files(
            &ctx.root,
            &["app/build.gradle", "app/build.gradle.kts",
              "build.gradle", "build.gradle.kts",
              "gradle/wrapper/gradle-wrapper.properties"],
        )
    }

    fn pre_build(&self, ctx: &BuildCtx) -> Result<()> {
        // Ensure gradlew is executable
        let gradlew = ctx.root.join("gradlew");
        if gradlew.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o755);
                std::fs::set_permissions(&gradlew, perms)?;
            }
        }
        Ok(())
    }
}

impl AndroidAdapter {
    fn check_prerequisites(&self, ctx: &BuildCtx) -> Result<()> {
        let gradlew = ctx.root.join("gradlew");
        if !gradlew.exists() {
            return Err(ArkonError::BuildFailed(
                "gradlew not found — this doesn't look like an Android project".into()
            ));
        }

        // Verify ANDROID_SDK_ROOT or ANDROID_HOME is set
        let sdk_set = ctx.env.contains_key("ANDROID_SDK_ROOT")
            || ctx.env.contains_key("ANDROID_HOME")
            || std::env::var("ANDROID_SDK_ROOT").is_ok()
            || std::env::var("ANDROID_HOME").is_ok();

        if !sdk_set {
            return Err(ArkonError::BuildFailed(
                "ANDROID_SDK_ROOT not set — install Android SDK and set the env var.\n\
                 Store it with: arkon secrets set ANDROID_SDK_ROOT /path/to/sdk".into()
            ));
        }

        // Verify Java is available
        if std::process::Command::new("java").arg("-version").output().is_err() {
            return Err(ArkonError::BuildFailed(
                "java not found in PATH — Android builds require JDK 17+".into()
            ));
        }

        Ok(())
    }

    fn find_primary_artifact(&self, output_dir: &PathBuf) -> Option<PathBuf> {
        for ext in &["apk", "aab"] {
            for entry in walkdir::WalkDir::new(output_dir)
                .max_depth(3)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().and_then(|x| x.to_str()) == Some(ext)
                    && !e.path().to_string_lossy().contains("unsigned")
                })
            {
                return Some(entry.path().to_path_buf());
            }
        }
        None
    }
}
