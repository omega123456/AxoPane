use std::collections::HashMap;
use std::path::{Path, PathBuf};

use file_explorer_lib::trash::{
    delete_batch_with_adapter_for_tests, BatchCorrelation, StableFileIdentity, SystemTrashAdapter,
    TrashFacts, TrashSnapshotEntry,
};

#[derive(Default)]
struct FakeTrash {
    root: Option<PathBuf>,
    before: Vec<TrashSnapshotEntry>,
    after: Vec<TrashSnapshotEntry>,
    sources: HashMap<String, TrashFacts>,
    failures: HashMap<String, String>,
    snapshots: usize,
    transactions: usize,
    recorded: Vec<BatchCorrelation>,
}
impl SystemTrashAdapter for FakeTrash {
    fn home_trash_root(&mut self) -> Result<Option<PathBuf>, String> {
        Ok(self.root.clone())
    }
    fn snapshot(&mut self, _: &Path) -> Result<Vec<TrashSnapshotEntry>, String> {
        self.snapshots += 1;
        Ok(if self.snapshots == 1 {
            self.before.clone()
        } else {
            self.after.clone()
        })
    }
    fn source_facts(&mut self, path: &str) -> Result<TrashFacts, String> {
        self.sources
            .get(path)
            .cloned()
            .ok_or_else(|| "missing fake source".to_string())
    }
    fn delete(&mut self, path: &str) -> Result<(), String> {
        self.failures.get(path).cloned().map_or(Ok(()), Err)
    }
    fn now_seconds(&mut self) -> i64 {
        42
    }
    fn record_manifest(
        &mut self,
        _: &Path,
        entries: &[BatchCorrelation],
        _: i64,
    ) -> Result<(), String> {
        self.transactions += 1;
        self.recorded.extend_from_slice(entries);
        Ok(())
    }
}
fn facts(path: &str, identity: Option<(u64, u64)>, size: u64) -> TrashFacts {
    TrashFacts {
        path: path.to_string(),
        name: Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        stable_identity: identity.map(|(device, file)| StableFileIdentity { device, file }),
        is_dir: false,
        size_bytes: Some(size),
        modified_at: Some(7),
    }
}
fn entry(name: &str, facts: TrashFacts) -> TrashSnapshotEntry {
    TrashSnapshotEntry {
        trash_name: name.to_string(),
        facts,
    }
}

#[test]
fn batch_uses_two_snapshots_one_transaction_and_identity_correlation() {
    let a_path = "/source/a.txt".to_string();
    let b_path = "/source/b.txt".to_string();
    let a = facts(&a_path, Some((1, 10)), 4);
    let b = facts(&b_path, Some((1, 11)), 5);
    let mut fake = FakeTrash {
        root: Some(PathBuf::from("/fake/.Trash")),
        before: vec![entry(
            "finder.txt",
            facts("/ignored/finder.txt", Some((1, 9)), 1),
        )],
        after: vec![
            entry("finder.txt", facts("/ignored/finder.txt", Some((1, 9)), 1)),
            entry("a.txt", a.clone()),
            entry("b.txt", b.clone()),
        ],
        sources: HashMap::from([(a_path.clone(), a), (b_path.clone(), b)]),
        ..Default::default()
    };
    let result = delete_batch_with_adapter_for_tests(&mut fake, &[a_path, b_path]);
    assert_eq!(
        (
            fake.snapshots,
            fake.transactions,
            result.correlated.len(),
            result.unknown_sources.len()
        ),
        (2, 1, 2, 0)
    );
}

#[test]
fn unique_metadata_fallback_correlates_but_duplicates_and_concurrent_additions_stay_unknown() {
    let unique_path = "/source/unique.txt".to_string();
    let duplicate_path = "/source/duplicate.txt".to_string();
    let unique = facts(&unique_path, None, 3);
    let duplicate = facts(&duplicate_path, None, 9);
    let mut fake = FakeTrash {
        root: Some(PathBuf::from("/fake/.Trash")),
        after: vec![
            entry("unique.txt", unique.clone()),
            entry("duplicate.txt", duplicate.clone()),
            entry("duplicate.txt 2", duplicate.clone()),
        ],
        sources: HashMap::from([
            (unique_path.clone(), unique),
            (duplicate_path.clone(), duplicate),
        ]),
        ..Default::default()
    };
    let result = delete_batch_with_adapter_for_tests(
        &mut fake,
        &[unique_path.clone(), duplicate_path.clone()],
    );
    assert_eq!(result.correlated[0].original_path, unique_path);
    assert_eq!(result.unknown_sources, vec![duplicate_path]);
    assert_eq!(fake.transactions, 1);
}

#[test]
fn failures_and_unobservable_external_volume_never_create_provenance() {
    let good = "/volume/item.txt".to_string();
    let bad = "/volume/missing.txt".to_string();
    let mut fake = FakeTrash {
        sources: HashMap::from([
            (good.clone(), facts(&good, Some((2, 1)), 1)),
            (bad.clone(), facts(&bad, Some((2, 2)), 1)),
        ]),
        failures: HashMap::from([(bad.clone(), "delete failed".to_string())]),
        ..Default::default()
    };
    let result = delete_batch_with_adapter_for_tests(&mut fake, &[good.clone(), bad.clone()]);
    assert_eq!(result.unknown_sources, vec![good]);
    assert_eq!(result.failures, vec![(bad, "delete failed".to_string())]);
    assert_eq!((fake.snapshots, fake.transactions), (0, 0));
}
