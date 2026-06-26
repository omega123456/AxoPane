use file_explorer_lib::volumes::list_volumes;

#[test]
fn enumerates_test_fixture_volumes_under_test_utils() {
    let volumes = list_volumes();

    if cfg!(windows) {
        assert_eq!(volumes.len(), 3);
        assert_eq!(volumes[0].mount_root, "C:\\");
        assert_eq!(volumes[0].label, "Fixture Windows");
        assert!(!volumes[0].is_network);
        assert!(!volumes[0].is_removable);
        assert_eq!(volumes[1].mount_root, "D:\\");
        assert!(!volumes[1].is_network);
        assert!(volumes[1].is_removable);
        assert_eq!(volumes[2].mount_root, "Z:\\");
        assert!(volumes[2].is_network);
        assert!(!volumes[2].is_removable);
    } else {
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0].mount_root, "/");
        assert_eq!(volumes[0].label, "Fixture Root");
        assert!(!volumes[0].is_network);
        assert!(!volumes[0].is_removable);
        assert_eq!(volumes[1].mount_root, "/Volumes/fixture-network");
        assert!(volumes[1].is_network);
        assert!(!volumes[1].is_removable);
    }

    for volume in &volumes {
        assert!(!volume.mount_root.is_empty());
        assert!(volume.total_bytes >= volume.free_bytes);
    }
}
