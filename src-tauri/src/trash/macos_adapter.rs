//! Batch-safe boundary around macOS system Trash deletion.
//!
//! The system deletion authority stays `trash`/`NsFileManager`.  This module
//! only observes the browsable home Trash directory before and after a batch,
//! then writes proven correlations in one manifest transaction.  It never
//! guesses the origin of an externally-created or ambiguous entry.

use std::collections::HashSet;
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
use super::manifest;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StableFileIdentity {
    pub device: u64,
    pub file: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashFacts {
    pub path: String,
    pub name: String,
    pub stable_identity: Option<StableFileIdentity>,
    pub is_dir: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashSnapshotEntry {
    pub trash_name: String,
    pub facts: TrashFacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchCorrelation {
    pub trashed_name: String,
    pub original_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchDeletionResult {
    pub correlated: Vec<BatchCorrelation>,
    pub unknown_sources: Vec<String>,
    pub failures: Vec<(String, String)>,
}

/// Injectable boundary so tests cannot reach `NsFileManager`, the real home
/// Trash, `.DS_Store`, or the system clock.
pub trait SystemTrashAdapter {
    fn home_trash_root(&mut self) -> Result<Option<PathBuf>, String>;
    fn snapshot(&mut self, root: &Path) -> Result<Vec<TrashSnapshotEntry>, String>;
    fn source_facts(&mut self, path: &str) -> Result<TrashFacts, String>;
    fn delete(&mut self, path: &str) -> Result<(), String>;
    fn now_seconds(&mut self) -> i64;
    fn record_manifest(
        &mut self,
        root: &Path,
        correlations: &[BatchCorrelation],
        deleted_at: i64,
    ) -> Result<(), String>;
}

/// Deletes a batch using exactly one before and one after snapshot when the
/// current home Trash root is observable.  A deletion can still succeed when
/// the item lands in another volume's system Trash; it simply remains unknown
/// to this home-root manifest instead of receiving fabricated provenance.
pub fn delete_batch<A: SystemTrashAdapter>(
    adapter: &mut A,
    paths: &[String],
) -> BatchDeletionResult {
    let Ok(root) = adapter.home_trash_root() else {
        return delete_without_observable_root(adapter, paths);
    };
    let Some(root) = root else {
        return delete_without_observable_root(adapter, paths);
    };

    let sources: Vec<_> = paths
        .iter()
        .map(|path| (path.clone(), adapter.source_facts(path)))
        .collect();
    let before = adapter.snapshot(&root).unwrap_or_default();
    let mut result = BatchDeletionResult::default();
    let mut successful_sources = Vec::new();
    for (path, facts) in sources {
        match facts {
            Err(error) => result.failures.push((path, error)),
            Ok(facts) => match adapter.delete(&path) {
                Ok(()) => successful_sources.push(facts),
                Err(error) => result.failures.push((path, error)),
            },
        }
    }

    let after = adapter.snapshot(&root).unwrap_or_default();
    let correlations = correlate(&before, &after, &successful_sources);
    let known: HashSet<_> = correlations
        .iter()
        .map(|item| item.original_path.as_str())
        .collect();
    result.unknown_sources.extend(
        successful_sources
            .iter()
            .filter(|source| !known.contains(source.path.as_str()))
            .map(|source| source.path.clone()),
    );
    result.correlated = correlations;
    if !result.correlated.is_empty() {
        let deleted_at = adapter.now_seconds();
        if let Err(error) = adapter.record_manifest(&root, &result.correlated, deleted_at) {
            // Deletions remain successful; making their origin unknown is
            // safer than reporting a failed delete after NsFileManager moved it.
            result
                .unknown_sources
                .extend(result.correlated.drain(..).map(|item| item.original_path));
            log::warn!("Failed to record batch Trash manifest: {error}");
        }
    }
    result
}

fn delete_without_observable_root<A: SystemTrashAdapter>(
    adapter: &mut A,
    paths: &[String],
) -> BatchDeletionResult {
    let mut result = BatchDeletionResult::default();
    for path in paths {
        match adapter.delete(path) {
            Ok(()) => result.unknown_sources.push(path.clone()),
            Err(error) => result.failures.push((path.clone(), error)),
        }
    }
    result
}

/// Correlates only entries created during this batch.  Stable identity wins;
/// the weaker metadata key is accepted only when it identifies exactly one
/// source and exactly one candidate.  Used candidates are consumed, making
/// every returned association one-to-one.
pub fn correlate(
    before: &[TrashSnapshotEntry],
    after: &[TrashSnapshotEntry],
    sources: &[TrashFacts],
) -> Vec<BatchCorrelation> {
    let before_names: HashSet<_> = before
        .iter()
        .map(|entry| entry.trash_name.as_str())
        .collect();
    let candidates: Vec<_> = after
        .iter()
        .filter(|entry| !before_names.contains(entry.trash_name.as_str()))
        .collect();
    let mut used = HashSet::new();
    let mut result = Vec::new();
    for source in sources {
        let identity_matches: Vec<_> =
            source
                .stable_identity
                .as_ref()
                .map_or_else(Vec::new, |identity| {
                    candidates
                        .iter()
                        .enumerate()
                        .filter_map(|(index, candidate)| {
                            (!used.contains(&index)
                                && candidate.facts.stable_identity.as_ref() == Some(identity))
                            .then_some((index, *candidate))
                        })
                        .collect()
                });
        let matches = if identity_matches.len() == 1 {
            identity_matches
        } else {
            candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| {
                    (!used.contains(&index) && compatible(source, &candidate.facts))
                        .then_some((index, *candidate))
                })
                .collect()
        };
        if matches.len() == 1 {
            let (index, candidate) = matches[0];
            used.insert(index);
            result.push(BatchCorrelation {
                trashed_name: candidate.trash_name.clone(),
                original_path: source.path.clone(),
            });
        }
    }
    result
}

fn compatible(source: &TrashFacts, candidate: &TrashFacts) -> bool {
    source.name == candidate.name
        && source.is_dir == candidate.is_dir
        && source.size_bytes == candidate.size_bytes
        && source.modified_at == candidate.modified_at
}

/// Production adapter.  It is intentionally private to this macOS module;
/// tests always supply their own fake adapter.
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub struct MacosSystemTrashAdapter {
    context: trash::TrashContext,
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
impl MacosSystemTrashAdapter {
    pub fn new() -> Self {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = trash::TrashContext::default();
        context.set_delete_method(DeleteMethod::NsFileManager);
        Self { context }
    }
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
impl SystemTrashAdapter for MacosSystemTrashAdapter {
    fn home_trash_root(&mut self) -> Result<Option<PathBuf>, String> {
        super::macos_bin::home_trash_dir().map(Some)
    }
    fn snapshot(&mut self, root: &Path) -> Result<Vec<TrashSnapshotEntry>, String> {
        snapshot_directory(root)
    }
    fn source_facts(&mut self, path: &str) -> Result<TrashFacts, String> {
        facts_for_path(Path::new(path), path.to_string())
    }
    fn delete(&mut self, path: &str) -> Result<(), String> {
        self.context.delete(path).map_err(|error| error.to_string())
    }
    fn now_seconds(&mut self) -> i64 {
        now_seconds()
    }
    fn record_manifest(
        &mut self,
        root: &Path,
        correlations: &[BatchCorrelation],
        deleted_at: i64,
    ) -> Result<(), String> {
        manifest::record_batch(
            root,
            correlations.iter().map(|item| {
                (
                    item.trashed_name.clone(),
                    item.original_path.clone(),
                    deleted_at,
                )
            }),
        )
    }
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn snapshot_directory(root: &Path) -> Result<Vec<TrashSnapshotEntry>, String> {
    let entries = fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (!manifest::is_manifest_file(&name))
                .then(|| {
                    facts_for_path(&entry.path(), entry.path().to_string_lossy().into_owned())
                        .ok()
                        .map(|facts| TrashSnapshotEntry {
                            trash_name: name,
                            facts,
                        })
                })
                .flatten()
        })
        .collect();
    Ok(entries)
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn facts_for_path(path: &Path, display_path: String) -> Result<TrashFacts, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    Ok(TrashFacts {
        path: display_path,
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        stable_identity: Some(StableFileIdentity {
            device: metadata.dev(),
            file: metadata.ino(),
        }),
        is_dir: metadata.is_dir(),
        size_bytes: (!metadata.is_dir()).then_some(metadata.len()),
        modified_at: Some(metadata.mtime()),
    })
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
