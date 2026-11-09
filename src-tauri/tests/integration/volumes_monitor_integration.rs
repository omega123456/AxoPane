use file_explorer_lib::volumes::{volume_inventory_changed, VolumeInfo};

struct VolumeDetails {
    total_bytes: u64,
    free_bytes: u64,
    is_network: bool,
    is_removable: bool,
}

fn fixed(total_bytes: u64, free_bytes: u64) -> VolumeDetails {
    VolumeDetails {
        total_bytes,
        free_bytes,
        is_network: false,
        is_removable: false,
    }
}

fn removable(total_bytes: u64, free_bytes: u64) -> VolumeDetails {
    VolumeDetails {
        is_removable: true,
        ..fixed(total_bytes, free_bytes)
    }
}

fn network(total_bytes: u64, free_bytes: u64) -> VolumeDetails {
    VolumeDetails {
        is_network: true,
        ..fixed(total_bytes, free_bytes)
    }
}

fn volume(mount_root: &str, label: &str, details: VolumeDetails) -> VolumeInfo {
    VolumeInfo {
        mount_root: mount_root.to_string(),
        label: label.to_string(),
        total_bytes: details.total_bytes,
        free_bytes: details.free_bytes,
        is_network: details.is_network,
        is_removable: details.is_removable,
    }
}

#[test]
fn ignores_free_space_only_changes_when_detecting_volume_inventory_updates() {
    let previous = vec![volume("C:\\", "Windows", fixed(1_000, 600))];
    let next = vec![volume("C:\\", "Windows", fixed(1_000, 550))];

    assert!(!volume_inventory_changed(&previous, &next));
}

#[test]
fn detects_added_removed_or_reidentified_mounts() {
    let previous = vec![
        volume("C:\\", "Windows", fixed(1_000, 600)),
        volume("D:\\", "USB", removable(500, 300)),
    ];

    let added = vec![
        volume("C:\\", "Windows", fixed(1_000, 600)),
        volume("D:\\", "USB", removable(500, 300)),
        volume("Z:\\", "Share", network(2_000, 1_500)),
    ];
    assert!(volume_inventory_changed(&previous, &added));

    let removed = vec![volume("C:\\", "Windows", fixed(1_000, 600))];
    assert!(volume_inventory_changed(&previous, &removed));

    let relabeled = vec![
        volume("C:\\", "Windows", fixed(1_000, 600)),
        volume("D:\\", "Backup USB", removable(500, 300)),
    ];
    assert!(volume_inventory_changed(&previous, &relabeled));
}

#[test]
fn compares_mounts_independent_of_input_order() {
    let previous = vec![
        volume("Z:\\", "Share", network(2_000, 1_500)),
        volume("C:\\", "Windows", fixed(1_000, 600)),
    ];
    let next = vec![
        volume("C:\\", "Windows", fixed(1_000, 550)),
        volume("Z:\\", "Share", network(2_000, 1_400)),
    ];

    assert!(!volume_inventory_changed(&previous, &next));
}
