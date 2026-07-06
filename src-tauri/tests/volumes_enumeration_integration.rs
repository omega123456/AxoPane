use file_explorer_lib::volumes::{list_volumes, path_is_network, VolumeInfo};

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
        assert_eq!(volumes.len(), 3);
        assert_eq!(volumes[0].mount_root, "/");
        assert_eq!(volumes[0].label, "Fixture Root");
        assert!(!volumes[0].is_network);
        assert!(!volumes[0].is_removable);
        assert_eq!(volumes[1].mount_root, "/Volumes/fixture-network");
        assert!(volumes[1].is_network);
        assert!(!volumes[1].is_removable);
        assert_eq!(volumes[2].mount_root, "/Volumes/Untitled");
        assert!(!volumes[2].is_network);
        assert!(volumes[2].is_removable);
    }

    for volume in &volumes {
        assert!(!volume.mount_root.is_empty());
        assert!(volume.total_bytes >= volume.free_bytes);
    }
}

#[test]
fn path_is_network_matches_longest_mount_root_across_platform_styles() {
    let volumes = vec![
        VolumeInfo {
            mount_root: if cfg!(windows) {
                "\\\\server\\share".to_string()
            } else {
                "/Volumes/team".to_string()
            },
            label: "network".to_string(),
            total_bytes: 0,
            free_bytes: 0,
            is_network: true,
            is_removable: false,
        },
        VolumeInfo {
            mount_root: if cfg!(windows) {
                "C:\\".to_string()
            } else {
                "/".to_string()
            },
            label: "local".to_string(),
            total_bytes: 0,
            free_bytes: 0,
            is_network: false,
            is_removable: false,
        },
    ];

    if cfg!(windows) {
        assert!(path_is_network(
            std::path::Path::new("\\\\server\\share\\folder\\file.txt"),
            &volumes
        ));
        assert!(!path_is_network(
            std::path::Path::new("C:\\Users\\Omega\\file.txt"),
            &volumes
        ));
    } else {
        assert!(path_is_network(
            std::path::Path::new("/Volumes/team/project/file.txt"),
            &volumes
        ));
        assert!(!path_is_network(
            std::path::Path::new("/Users/omega/file.txt"),
            &volumes
        ));
    }
}
