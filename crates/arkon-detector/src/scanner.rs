use crate::rules::RULES;
use arkon_core::error::{ArkonError, Result};
use std::path::{Path, PathBuf};

const MIN_CONFIDENCE: f32 = 0.6;

/// The result of a successful detection pass.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Adapter name to invoke, e.g. "nextjs", "unity", "python"
    pub adapter: String,
    /// Confidence score 0.0–1.0
    pub confidence: f32,
    /// Human-readable description of the matched rule
    pub description: String,
    /// All candidates sorted by confidence descending (for --verbose)
    pub all_candidates: Vec<Candidate>,
    /// Root directory that was scanned
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub adapter: String,
    pub confidence: f32,
    pub description: String,
}

/// Scores all built-in rules against a project root and returns the winner.
#[derive(Default)]
pub struct ProjectDetector {
    /// Minimum confidence threshold. Defaults to 0.6.
    pub min_confidence: Option<f32>,
}

impl ProjectDetector {
    pub fn with_min_confidence(mut self, c: f32) -> Self {
        self.min_confidence = Some(c);
        self
    }

    pub fn detect(&self, root: &Path) -> Result<DetectionResult> {
        let root = root
            .canonicalize()
            .map_err(|e| ArkonError::DetectionFailed(e.to_string()))?;

        let threshold = self.min_confidence.unwrap_or(MIN_CONFIDENCE);

        // Score every rule
        let mut candidates: Vec<Candidate> = RULES
            .iter()
            .map(|rule| Candidate {
                adapter: rule.adapter.to_string(),
                confidence: rule.score(&root),
                description: rule.description.to_string(),
            })
            .filter(|c| c.confidence > 0.0)
            .collect();

        // Sort descending by confidence, then adapter name for stability
        candidates.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.adapter.cmp(&b.adapter))
        });

        // Log all candidates at debug level
        for c in &candidates {
            tracing::debug!(
                adapter = %c.adapter,
                confidence = %c.confidence,
                desc = %c.description,
                "candidate"
            );
        }

        let winner = candidates
            .first()
            .filter(|c| c.confidence >= threshold)
            .ok_or_else(|| {
                ArkonError::DetectionFailed(format!(
                    "no adapter scored ≥ {threshold:.0}% in '{}'.\n\
                     Run `arkon detect --verbose` to see candidates.\n\
                     Set `adapter = \"<name>\"` in arkon.toml to force one.",
                    root.display()
                ))
            })?;

        Ok(DetectionResult {
            adapter: winner.adapter.clone(),
            confidence: winner.confidence,
            description: winner.description.clone(),
            all_candidates: candidates,
            root,
        })
    }
}

impl DetectionResult {
    /// Pretty-print the detection result to stdout.
    pub fn print_summary(&self) {
        let pct = (self.confidence * 100.0).round() as u8;
        println!(
            "  \x1b[32m✓\x1b[0m detected \x1b[1m{}\x1b[0m  \x1b[2m({} — {}%)\x1b[0m",
            self.adapter, self.description, pct
        );
    }

    /// Print all candidates — used by `arkon detect --verbose`.
    pub fn print_verbose(&self) {
        println!("\n  \x1b[2mAll candidates:\x1b[0m");
        for c in &self.all_candidates {
            let pct = (c.confidence * 100.0).round() as u8;
            let marker = if c.adapter == self.adapter { "›" } else { " " };
            println!(
                "  \x1b[33m{marker}\x1b[0m  {:20}  {:3}%  {}",
                c.adapter, pct, c.description
            );
        }
        println!();
    }
}
