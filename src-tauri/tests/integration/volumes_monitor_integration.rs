use file_explorer_lib::volumes::{volume_inventory_changed, VolumeInfo};

fn volume(
    mount_root: &str,
    label: &str,
    total_bytes: u64,
    free_bytes: u64,
    is_network: bool,
    is_removable: bool,
) -> VolumeInfo {
    VolumeInfo {
        mount_root: mount_root.to_string(),
        label: label.to_string(),
        total_bytes,
        free_bytes,
        is_network,
        is_removable,
    }
}

#[test]
fn ignores_free_space_only_changes_when_detecting_volume_inventory_updates() {
    let previous = vec![volume("C:\\", "Windows", 1_000, 600, false, false)];
    let next = vec![volume("C:\\", "Windows", 1_000, 550, false, false)];

    assert!(!volume_inventory_changed(&previous, &next));
}

#[test]
fn detects_added_removed_or_reidentified_mounts() {
    let previous = vec![
        volume("C:\\", "Windows", 1_000, 600, false, false),
        volume("D:\\", "USB", 500, 300, false, true),
    ];

    let added = vec![
        volume("C:\\", "Windows", 1_000, 600, false, false),
        volume("D:\\", "USB", 500, 300, false, true),
        volume("Z:\\", "Share", 2_000, 1_500, true, false),
    ];
    assert!(volume_inventory_changed(&previous, &added));

    let removed = vec![volume("C:\\", "Windows", 1_000, 600, false, false)];
    assert!(volume_inventory_changed(&previous, &removed));

    let relabeled = vec![
        volume("C:\\", "Windows", 1_000, 600, false, false),
        volume("D:\\", "Backup USB", 500, 300, false, true),
    ];
    assert!(volume_inventory_changed(&previous, &relabeled));
}

#[test]
fn compares_mounts_independent_of_input_order() {
    let previous = vec![
        volume("Z:\\", "Share", 2_000, 1_500, true, false),
        volume("C:\\", "Windows", 1_000, 600, false, false),
    ];
    let next = vec![
        volume("C:\\", "Windows", 1_000, 550, false, false),
        volume("Z:\\", "Share", 2_000, 1_400, true, false),
    ];

    assert!(!volume_inventory_changed(&previous, &next));
}
