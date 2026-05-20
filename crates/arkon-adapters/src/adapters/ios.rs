use crate::{Adapter, BuildCtx, fingerprint, runner};
use arkon_core::{
    artifact::{Artifact, DeployableKind},
    error::{ArkonError, Result},
    runtime::{Runtime, RuntimeKind},
};
use std::path::PathBuf;
use tracing::info;

/// iOS application adapter.
/// Requires macOS with Xcode installed.
/// Produces a `.ipa` file for distribution.
pub struct IosAdapter;

impl Adapter for IosAdapter {
    fn name(&self) -> &str { "ios" }
    fn description(&self) -> &str { "iOS application (xcodebuild → IPA)" }

    fn build(&self, ctx: &BuildCtx) -> Result<Artifact> {
        self.check_prerequisites(ctx)?;

        let scheme    = self.detect_scheme(ctx);
        let output    = self.output_dir(ctx);
        std::fs::create_dir_all(&output)?;

        // 1. Archive
        let archive_path = output.join("arkon-build.xcarchive");
        let archive_cmd  = format!(
            "xcodebuild archive -scheme '{scheme}' \
             -archivePath '{}' \
             -configuration Release \
             CODE_SIGN_IDENTITY='' \
             CODE_SIGNING_REQUIRED=NO \
             CODE_SIGNING_ALLOWED=NO",
            archive_path.display()
        );

        info!(scheme = %scheme, "archiving iOS app");
        runner::run_shell_command(&archive_cmd, &ctx.root, &ctx.env)?;

        if !archive_path.exists() {
            return Err(ArkonError::BuildFailed(format!(
                "xcarchive not created at '{}' — check xcodebuild output",
                archive_path.display()
            )));
        }

        // 2. Export to IPA
        let export_dir     = output.join("ipa");
        let export_options = self.write_export_options(ctx, &output)?;
        let export_cmd     = format!(
            "xcodebuild -exportArchive \
             -archivePath '{}' \
             -exportPath '{}' \
             -exportOptionsPlist '{}'",
            archive_path.display(),
            export_dir.display(),
            export_options.display()
        );

        info!("exporting IPA");
        runner::run_shell_command(&export_cmd, &ctx.root, &ctx.env)?;

        let (fp, hashes, size) = fingerprint::fingerprint_dir(&export_dir)?;
        let mut art = Artifact::new("ios", export_dir, DeployableKind::Binary);
        art.fingerprint = fp;
        art.file_hashes = hashes;
        art.size_bytes  = size;

        if let Some(ipa) = self.find_ipa(&art.path) {
            art = art.with_meta("ipa_path", ipa.to_string_lossy().as_ref());
        }

        Ok(art)
    }

    fn output_dir(&self, ctx: &BuildCtx) -> PathBuf {
        if let Some(d) = &ctx.output_dir_override { return ctx.root.join(d); }
        ctx.root.join("build/arkon-ios")
    }

    fn runtime_info(&self) -> Runtime {
        Runtime {
            kind:     RuntimeKind::Native,
            version:  None,
            env_keys: vec!["APPLE_TEAM_ID".into(), "CODE_SIGN_IDENTITY".into()],
        }
    }

    fn deployable_type(&self) -> DeployableKind { DeployableKind::Binary }

    fn cache_key(&self, ctx: &BuildCtx) -> String {
        // Find the first .xcodeproj or .xcworkspace for cache key
        let proj_files: Vec<&str> = vec![
            "*.xcodeproj/project.pbxproj",
            "Podfile.lock",
            "Package.resolved",
        ];
        fingerprint::cache_key_from_files(&ctx.root, &proj_files)
    }
}

impl IosAdapter {
    fn check_prerequisites(&self, _ctx: &BuildCtx) -> Result<()> {
        // Must be macOS
        #[cfg(not(target_os = "macos"))]
        {
            return Err(ArkonError::BuildFailed(
                "iOS builds require macOS with Xcode installed".into()
            ));
        }

        // Check xcodebuild exists
        if std::process::Command::new("xcodebuild").arg("-version").output().is_err() {
            return Err(ArkonError::BuildFailed(
                "xcodebuild not found — install Xcode from the App Store".into()
            ));
        }

        Ok(())
    }

    fn detect_scheme(&self, ctx: &BuildCtx) -> String {
        // Try to find the scheme from .xcodeproj
        for entry in walkdir::WalkDir::new(&ctx.root)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("xcodeproj"))
        {
            let name = entry.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("App");
            return name.to_string();
        }
        "App".to_string()
    }

    fn write_export_options(&self, ctx: &BuildCtx, output: &PathBuf) -> Result<PathBuf> {
        let team_id = ctx.env.get("APPLE_TEAM_ID")
            .cloned()
            .unwrap_or_else(|| "XXXXXXXXXX".into());

        let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key>
    <string>development</string>
    <key>teamID</key>
    <string>{team_id}</string>
    <key>compileBitcode</key>
    <false/>
    <key>uploadSymbols</key>
    <false/>
</dict>
</plist>"#);

        let path = output.join("ExportOptions.plist");
        std::fs::write(&path, plist)?;
        Ok(path)
    }

    fn find_ipa(&self, dir: &PathBuf) -> Option<PathBuf> {
        walkdir::WalkDir::new(dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .find(|e| e.path().extension().and_then(|x| x.to_str()) == Some("ipa"))
            .map(|e| e.path().to_path_buf())
    }
}
