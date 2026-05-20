mod rules;
mod scanner;

pub use rules::DetectionRule;
pub use scanner::{DetectionResult, ProjectDetector};

use arkon_core::error::{ArkonError, Result};
use std::path::Path;

/// Detect the project type at `root` and return the best adapter name.
/// Returns an error if no adapter scores ≥ 0.6.
pub fn detect(root: &Path) -> Result<DetectionResult> {
    let detector = ProjectDetector::default();
    detector.detect(root)
}

/// Detect and print a human-readable summary to stdout.
pub fn detect_and_report(root: &Path) -> Result<DetectionResult> {
    let result = detect(root)?;
    tracing::info!(
        adapter = %result.adapter,
        confidence = %result.confidence,
        "project detected"
    );
    Ok(result)
}
