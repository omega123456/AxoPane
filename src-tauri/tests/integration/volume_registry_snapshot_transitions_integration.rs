use file_explorer_lib::volumes::registry::RegistrySnapshot;

const CAPTURED_AT_MS: u64 = 1_000;
const LATER_MS: u64 = 2_000;
const LATEST_MS: u64 = 3_000;

#[test]
fn refresh_flag_and_staleness_produce_new_snapshots() {
    let snapshot = RegistrySnapshot::empty(CAPTURED_AT_MS);
    let refreshing = snapshot.with_refresh_in_flight(true);
    assert!(refreshing.refresh_in_flight);
    assert!(!snapshot.refresh_in_flight);

    let stale = snapshot.marked_stale(LATER_MS);
    assert_eq!(stale.stale_since_ms, Some(LATER_MS));
    assert_eq!(stale.marked_stale(LATEST_MS).stale_since_ms, Some(LATER_MS));
    assert_eq!(snapshot.stale_since_ms, None);
}

#[test]
fn reobserving_clears_staleness_and_keeps_the_revision() {
    let stale = RegistrySnapshot::empty(CAPTURED_AT_MS).marked_stale(LATER_MS);
    let reobserved = stale.reobserved(LATEST_MS);
    assert_eq!(reobserved.stale_since_ms, None);
    assert_eq!(reobserved.captured_at_ms, LATEST_MS);
    assert_eq!(reobserved.revision, stale.revision);
}

#[test]
fn superseding_bumps_the_revision_and_carries_the_refresh_flag() {
    let current = RegistrySnapshot::empty(CAPTURED_AT_MS)
        .with_refresh_in_flight(true)
        .marked_stale(LATER_MS);
    let candidate = RegistrySnapshot::empty(LATEST_MS).marked_stale(LATEST_MS);
    let published = current.superseded_by(candidate);
    assert_eq!(published.revision, current.revision + 1);
    assert_eq!(published.captured_at_ms, LATEST_MS);
    assert_eq!(published.stale_since_ms, None);
    assert!(published.refresh_in_flight);
}
