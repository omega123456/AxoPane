use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::EjectVolumeRequest;

#[test]
fn eject_volume_is_unsupported_under_test_utils_for_a_removable_fixture() {
    let removable_mount_root = if cfg!(windows) {
        "D:\\".to_string()
    } else {
        "/Volumes/Untitled".to_string()
    };

    let status = commands::eject_volume(EjectVolumeRequest {
        mount_root: removable_mount_root,
    })
    .expect("eject volume");

    assert!(!status.handled);
    assert_eq!(status.message.as_deref(), Some("unsupported"));
}

#[test]
fn eject_volume_is_unsupported_under_test_utils_for_an_arbitrary_mount_root() {
    let status = commands::eject_volume(EjectVolumeRequest {
        mount_root: "not-a-real-mount-root".to_string(),
    })
    .expect("eject volume");

    assert!(!status.handled);
    assert_eq!(status.message.as_deref(), Some("unsupported"));
}
