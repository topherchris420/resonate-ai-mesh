//! Golden recordings: committed run bundles that CI replays.
//!
//! Each fixture directory holds a full run bundle and a `source.yaml` that
//! says how to regenerate it. `mesh golden verify` replays every bundle; if
//! the code's deterministic output changed, verification fails and reports the
//! first divergent event. `mesh golden update` regenerates bundles after an
//! intentional change.

use crate::bundle::{save_run, SaveOptions};
use crate::record::Bundle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenSource {
    /// Scenario or manifest, relative to the fixture directory.
    pub path: PathBuf,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub rep: u32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    /// False for recordings that cannot be remade offline, such as a run
    /// judged by a remote model. `golden update` leaves them untouched;
    /// `golden verify` still replays them.
    #[serde(default = "regenerable")]
    pub regenerate: bool,
}

fn regenerable() -> bool {
    true
}

pub fn fixture_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.join("manifest.json").is_file() || p.join("source.yaml").is_file())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

pub struct GoldenResult {
    pub name: String,
    pub report: Option<crate::replay::ReplayReport>,
    pub error: Option<String>,
}

pub async fn verify(root: &Path) -> Vec<GoldenResult> {
    let mut results = Vec::new();
    for dir in fixture_dirs(root) {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match Bundle::load(&dir) {
            Ok(bundle) => results.push(GoldenResult {
                name,
                report: Some(crate::replay::replay_bundle(&bundle).await),
                error: None,
            }),
            Err(error) => results.push(GoldenResult {
                name,
                report: None,
                error: Some(error.to_string()),
            }),
        }
    }
    results
}

pub async fn update(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut updated = Vec::new();
    for dir in fixture_dirs(root) {
        let source_path = dir.join("source.yaml");
        if !source_path.is_file() {
            continue;
        }
        let source: GoldenSource =
            crate::config::read_yaml(&source_path).map_err(|e| e.to_string())?;
        if !source.regenerate {
            continue;
        }
        let target_path = dir.join(&source.path);
        let target = crate::cli::load_target(&target_path)?;
        let config = crate::cli::resolve_target(
            &target,
            source.condition.as_deref(),
            source.rep,
            source.seed,
            &source.set,
        )?;
        let result = crate::runner::run(&config, crate::runner::RunOptions::default())
            .await
            .map_err(|e| e.to_string())?;
        save_run(
            &result,
            &dir,
            SaveOptions {
                force: true,
                command: vec!["mesh".into(), "golden".into(), "update".into()],
                reproduce: vec![format!("mesh golden update --dir {}", root.display())],
                parent: None,
                started_at_wall: chrono::Utc::now().to_rfc3339(),
                limitations: &[],
            },
        )
        .map_err(|e| e.to_string())?;
        updated.push((config.run_id.clone(), result.head_hash.clone()));
    }
    Ok(updated)
}
