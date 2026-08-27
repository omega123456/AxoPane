//! Integration coverage for the shared `VolumeRegistry` introduced to
//! replace per-call platform volume discovery and short-lived watcher
//! registrations with one long-lived, revisioned source of volume identity.
//!
//! These tests only exercise the `test-utils` fake surface
//! (`VolumeRegistry::set_fake_inventory` + `refresh`): no real Disk
//! Arbitration session or Windows shell-notification watcher is ever
//! started, matching the repository rule that tests must never touch real
//! machine-global registrations.

use std::sync::Arc;

use file_explorer_lib::volumes::registry::{
    RegistrySnapshot, VolumeAvailability, VolumeRegistry, VolumeResourceKind,
};
use file_explorer_lib::volumes::VolumeInfo;

fn volume(mount_root: &str, label: &str, is_network: bool, is_removable: bool) -> VolumeInfo {
    VolumeInfo {
        mount_root: mount_root.to_string(),
        label: label.to_string(),
        total_bytes: 1_000_000,
        free_bytes: 500_000,
        is_network,
        is_removable,
    }
}

fn new_registry() -> Arc<VolumeRegistry> {
    let registry = Arc::new(VolumeRegistry::default());
    registry.start(());
    registry
}

#[test]
fn start_publishes_an_initial_snapshot_without_requiring_a_refresh_call() {
    let registry = new_registry();
    let snapshot = registry.snapshot();

    // `start` seeds the first snapshot synchronously so callers never
    // observe an empty registry immediately after startup.
    assert!(!snapshot.volumes.is_empty());
    assert_eq!(snapshot.revision, 1);
}

#[test]
fn repeated_snapshot_reads_do_not_change_revision_or_perform_a_refresh() {
    let registry = new_registry();
    let first = registry.snapshot();

    for _ in 0..25 {
        let again = registry.snapshot();
        assert_eq!(again.revision, first.revision);
        assert_eq!(again.volumes, first.volumes);
    }
}

#[test]
fn simulated_add_event_increments_revision_and_updates_the_snapshot() {
    let registry = new_registry();
    let before = registry.snapshot();

    let mut inventory = before.to_volume_infos();
    inventory.push(volume("/Volumes/added-drive", "Added Drive", false, true));
    registry.set_fake_inventory(inventory);

    let after = registry.refresh();

    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(after.volumes.len(), before.volumes.len() + 1);
    assert!(after
        .volumes
        .iter()
        .any(|v| v.primary_path == "/Volumes/added-drive"));
}

#[test]
fn simulated_remove_event_increments_revision_and_drops_the_volume() {
    let registry = new_registry();
    let before = registry.snapshot();
    assert!(
        before.volumes.len() > 1,
        "fixture must start with >1 volume"
    );

    let mut inventory = before.to_volume_infos();
    inventory.remove(inventory.len() - 1);
    registry.set_fake_inventory(inventory);

    let after = registry.refresh();

    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(after.volumes.len(), before.volumes.len() - 1);
}

#[test]
fn simulated_change_event_relabeling_a_volume_increments_revision() {
    let registry = new_registry();
    let before = registry.snapshot();

    let mut inventory = before.to_volume_infos();
    inventory[0].label = format!("{}-renamed", inventory[0].label);
    registry.set_fake_inventory(inventory);

    let after = registry.refresh();

    assert_eq!(after.revision, before.revision + 1);
    assert_ne!(after.volumes[0].label, before.volumes[0].label);
}

#[test]
fn refresh_with_no_inventory_change_does_not_bump_revision() {
    let registry = new_registry();
    let before = registry.snapshot();

    // No `set_fake_inventory` call: the fake platform inventory is
    // unchanged, so `refresh` must be a no-op with respect to revision.
    let after = registry.refresh();

    assert_eq!(after.revision, before.revision);
    assert_eq!(after.volumes, before.volumes);
}

#[test]
fn refresh_clears_refresh_in_flight_and_never_reports_it_stuck_true() {
    let registry = new_registry();
    let snapshot = registry.refresh();
    assert!(!snapshot.refresh_in_flight);

    // A snapshot read outside of `refresh` must also never report a
    // stuck in-flight refresh under test-utils, since the fake refresh
    // path is synchronous.
    assert!(!registry.snapshot().refresh_in_flight);
}

#[test]
fn resolves_windows_style_unc_path_across_short_and_long_alias_forms() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![volume(
        "\\\\build-server\\shared-drop",
        "Shared Drop",
        true,
        false,
    )]);
    let snapshot = registry.refresh();

    let via_plain = snapshot.resolve("\\\\build-server\\shared-drop\\artifacts\\out.zip");
    let via_long = snapshot.resolve("\\\\?\\UNC\\build-server\\shared-drop\\artifacts\\out.zip");

    let plain = via_plain.expect("plain UNC alias should resolve");
    let long = via_long.expect("long UNC alias should resolve");

    assert_eq!(plain.resource_key, long.resource_key);
    assert_eq!(plain.kind, VolumeResourceKind::UncShare);
}

#[test]
fn resolves_exact_match_before_longest_mount_containment() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![
        volume("/Volumes/data", "Data", false, false),
        volume("/Volumes/data/nested", "Nested", false, false),
    ]);
    let snapshot = registry.refresh();

    // A path exactly equal to the shorter mount root must resolve to that
    // mount, not be reinterpreted as "contained within" a deeper one.
    let exact = snapshot.resolve("/Volumes/data").expect("exact match");
    assert_eq!(exact.primary_path, "/Volumes/data");

    // A path under the nested mount resolves to the longest (most
    // specific) containing root.
    let nested = snapshot
        .resolve("/Volumes/data/nested/file.txt")
        .expect("nested containment match");
    assert_eq!(nested.primary_path, "/Volumes/data/nested");
}

#[test]
fn resolves_case_insensitive_only_as_a_compatibility_fallback() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![volume("C:\\Data", "Data", false, false)]);
    let snapshot = registry.refresh();

    // Exact-case match resolves directly.
    assert!(snapshot.resolve("C:\\Data\\file.txt").is_some());

    // A differently-cased path only resolves via the case-insensitive
    // fallback tier, and resolves to the *same* record.
    let fallback = snapshot
        .resolve("c:\\data\\FILE.TXT")
        .expect("case-insensitive fallback should still resolve");
    assert_eq!(fallback.primary_path, "C:\\Data");
}

#[test]
fn resolve_returns_none_for_a_path_outside_every_known_mount() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![volume("/Volumes/data", "Data", false, false)]);
    let snapshot = registry.refresh();

    assert!(snapshot.resolve("/Volumes/unrelated/file.txt").is_none());
}

#[test]
fn resolve_resource_key_matches_snapshot_resolve() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![volume("/Volumes/data", "Data", false, false)]);
    registry.refresh();

    let key = registry
        .resolve_resource_key("/Volumes/data/subdir")
        .expect("resource key should resolve");
    let snapshot = registry.snapshot();
    let expected = snapshot
        .resolve("/Volumes/data/subdir")
        .expect("snapshot resolve should also match")
        .resource_key
        .clone();

    assert_eq!(key, expected);
}

#[test]
fn every_fallback_root_volume_is_explicitly_marked_as_fallback_kind() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![volume("/Volumes/data", "Data", false, false)]);
    let snapshot = registry.refresh();

    // A plain POSIX mount root (no UNC form) must never claim a stronger
    // GUID/UUID identity than the registry can actually confirm.
    let record = &snapshot.volumes[0];
    assert_eq!(record.kind, VolumeResourceKind::FallbackRoot);
    assert!(record.resource_key.starts_with("fallback:"));
}

#[test]
fn snapshot_volumes_carry_available_status_and_a_validation_timestamp() {
    let registry = new_registry();
    let snapshot = registry.refresh();

    for volume in &snapshot.volumes {
        assert_eq!(volume.availability, VolumeAvailability::Available);
        assert!(volume.last_validated_at_ms > 0);
    }
}

#[test]
fn to_volume_infos_preserves_legacy_ipc_shape_and_order() {
    let registry = new_registry();
    let infos = registry.snapshot().to_volume_infos();
    let fixture = file_explorer_lib::volumes::list_volumes();

    assert_eq!(infos.len(), fixture.len());
    for (info, expected) in infos.iter().zip(fixture.iter()) {
        assert_eq!(info.mount_root, expected.mount_root);
        assert_eq!(info.label, expected.label);
        assert_eq!(info.is_network, expected.is_network);
        assert_eq!(info.is_removable, expected.is_removable);
    }
}

#[test]
fn snapshot_is_serializable_round_trip_for_the_ipc_boundary() {
    let registry = new_registry();
    let snapshot = registry.snapshot();

    let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
    let round_tripped: RegistrySnapshot =
        serde_json::from_str(&json).expect("snapshot should deserialize");

    assert_eq!(round_tripped, snapshot);
}

#[test]
fn concurrent_refresh_calls_settle_on_one_coherent_final_snapshot() {
    let registry = new_registry();
    registry.set_fake_inventory(vec![
        volume("/Volumes/a", "A", false, false),
        volume("/Volumes/b", "B", false, false),
    ]);

    let mut handles = Vec::new();
    for _ in 0..8 {
        let registry = Arc::clone(&registry);
        handles.push(std::thread::spawn(move || registry.refresh()));
    }

    for handle in handles {
        handle.join().expect("refresh thread should not panic");
    }

    // Whatever interleaving occurred, exactly one coherent final snapshot
    // is left, matching the fake inventory's final state, with a revision
    // that only ever moved forward.
    let final_snapshot = registry.snapshot();
    assert_eq!(final_snapshot.volumes.len(), 2);
    assert!(final_snapshot.revision >= 1);
}
