#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::time::Duration;

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    FolderSizeRequest, FolderSizesRequest, RefreshTabRequest, SetTabWatchRequest, WatchTarget,
};
use file_explorer_lib::ops::OpsService;
use file_explorer_lib::persist::PersistenceState;
use file_explorer_lib::size::SizeService;
use file_explorer_lib::watch::WatchService;
use tauri::Manager;
use tempfile::tempdir;

fn build_app() -> (tempfile::TempDir, tauri::App) {
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");

    let app = tauri::Builder::default()
        .manage(persistence)
        .manage(SizeService::default())
        .manage(WatchService::default())
        .manage(OpsService::new(Duration::from_secs(30)))
        .build(tauri::generate_context!())
        .expect("build app");

    (config_dir, app)
}

#[test]
fn concrete_apphandle_commands_cover_volume_watch_and_size_wrappers() {
    let (_config_dir, app) = build_app();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("before.txt"), b"before").expect("before");

    let volumes = commands::list_volumes(app.handle().clone()).expect("list volumes");
    assert!(!volumes.is_empty());
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let watch_target = WatchTarget {
        tab_id: "left-1".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    };

    commands::set_tab_watch(
        SetTabWatchRequest {
            target: Some(watch_target.clone()),
        },
        app.handle().clone(),
        app.state::<WatchService>(),
    )
    .expect("set watch");

    fs::remove_file(root.join("before.txt")).expect("remove");
    fs::write(root.join("after.txt"), b"after").expect("after");

    let patch = commands::refresh_tab(
        RefreshTabRequest {
            target: watch_target.clone(),
        },
        app.handle().clone(),
        app.state::<WatchService>(),
    )
    .expect("refresh");
    assert!(patch
        .removed
        .iter()
        .any(|path| path.ends_with("before.txt")));
    assert!(patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("after.txt")));

    commands::set_tab_watch(
        SetTabWatchRequest { target: None },
        app.handle().clone(),
        app.state::<WatchService>(),
    )
    .expect("clear watch");

    let size_root = root.join("sizes");
    fs::create_dir_all(&size_root).expect("sizes dir");
    for index in 0..20 {
        fs::write(size_root.join(format!("file-{index}.txt")), b"1234567890").expect("file");
    }

    commands::request_folder_size(
        FolderSizeRequest {
            path: size_root.to_string_lossy().into_owned(),
        },
        app.handle().clone(),
        app.state::<SizeService>(),
    );
    commands::request_folder_sizes(
        FolderSizesRequest {
            paths: vec![size_root.to_string_lossy().into_owned()],
        },
        app.handle().clone(),
        app.state::<SizeService>(),
    );
}
