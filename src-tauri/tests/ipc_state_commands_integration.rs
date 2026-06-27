#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    CancelSizeRequest, ConflictResolution, CreateEntryRequest, DeleteEntriesRequest,
    LogFrontendRequest, OpIdRequest, OpenPathRequest, ReorderOpsRequest, ResolveConflictRequest,
    SaveConfigRequest, SaveSessionRequest,
};
use file_explorer_lib::ops::{OpItem, OpKind, OpStatus, OpsService, StartOpRequest};
use file_explorer_lib::persist::{Config, PersistenceState, Session};
use file_explorer_lib::size::SizeService;
use file_explorer_lib::size::{size_path_from_string, EverythingStatusKind, SizeStateKind};
use file_explorer_lib::watch::WatchService;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Manager, Runtime};
use tempfile::tempdir;

struct TestApp<R: Runtime> {
    _config_dir: tempfile::TempDir,
    app: tauri::App<R>,
}

impl TestApp<tauri::test::MockRuntime> {
    fn new() -> Self {
        let config_dir = tempdir().expect("config dir");
        let persistence = PersistenceState::load(config_dir.path()).expect("persistence");

        let app = mock_builder()
            .manage(persistence)
            .manage(SizeService::default())
            .manage(WatchService::default())
            .manage(OpsService::new(Duration::from_secs(30)))
            .build(mock_context(noop_assets()))
            .expect("mock app");

        Self {
            _config_dir: config_dir,
            app,
        }
    }

    fn wait_for_op(&self, id: &str, predicate: impl Fn(OpStatus) -> bool) -> OpStatus {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = commands::queue_snapshot(self.app.state::<OpsService>())
                .into_iter()
                .find(|snapshot| snapshot.progress.operation_id == id)
                .map(|snapshot| snapshot.progress.status);

            if let Some(status) = status {
                if predicate(status) {
                    return status;
                }
            }

            assert!(Instant::now() < deadline, "timed out waiting for op {id}");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn op_item(path: &Path) -> OpItem {
    let metadata = fs::metadata(path).expect("metadata");
    OpItem {
        source_path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned(),
        size_bytes: metadata.len(),
    }
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, contents).expect("write");
}

#[test]
fn commands_cover_filesystem_and_persistence_flows() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let shell = commands::get_initial_shell();
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let initial_config = commands::load_config(test_app.app.state::<PersistenceState>());
    assert_eq!(initial_config.theme, "system");

    let saved_config = Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        dismissed_everything_banner: true,
        keybindings: initial_config.keybindings.clone(),
        columns: initial_config.columns.clone(),
        layout: initial_config.layout.clone(),
        update_check_interval: "12h".to_string(),
    };
    assert_eq!(
        commands::save_config(
            SaveConfigRequest {
                config: saved_config.clone(),
            },
            test_app.app.state::<PersistenceState>(),
        ),
        saved_config
    );
    assert_eq!(
        commands::load_config(test_app.app.state::<PersistenceState>()),
        saved_config
    );

    let saved_session = Session {
        active_pane: "right".to_string(),
        left_path: root.join("left").to_string_lossy().into_owned(),
        right_path: root.join("right").to_string_lossy().into_owned(),
        left: None,
        right: None,
    };
    assert_eq!(
        commands::save_session(
            SaveSessionRequest {
                session: saved_session.clone(),
            },
            test_app.app.state::<PersistenceState>(),
        ),
        saved_session
    );
    assert_eq!(
        commands::load_session(test_app.app.state::<PersistenceState>()),
        saved_session
    );

    let folder = commands::create_folder(CreateEntryRequest {
        parent: root.to_string_lossy().into_owned(),
        name: "Docs".to_string(),
    })
    .expect("create folder");
    assert!(folder.is_dir);

    let file = commands::create_file(CreateEntryRequest {
        parent: root.to_string_lossy().into_owned(),
        name: "todo.txt".to_string(),
    })
    .expect("create file");
    assert_eq!(file.name, "todo.txt");

    let listing = commands::list_dir(file_explorer_lib::ipc::types::ListDirRequest {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    })
    .expect("list dir");
    assert_eq!(listing.entries.len(), 2);

    let renamed = commands::rename_entry(file_explorer_lib::ipc::types::RenameEntryRequest {
        path: root.join("todo.txt").to_string_lossy().into_owned(),
        new_name: "done.txt".to_string(),
    })
    .expect("rename");
    assert_eq!(renamed.name, "done.txt");

    commands::delete_entries(DeleteEntriesRequest {
        paths: vec![
            root.join("Docs").to_string_lossy().into_owned(),
            root.join("done.txt").to_string_lossy().into_owned(),
        ],
    })
    .expect("delete entries");
    assert!(!root.join("Docs").exists());
    assert!(!root.join("done.txt").exists());

    let missing_dir_error = commands::list_dir(file_explorer_lib::ipc::types::ListDirRequest {
        path: root.join("missing").to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    })
    .expect_err("missing directory");
    assert!(missing_dir_error.contains("Failed to load"));

    let missing_path_error = commands::open_path(OpenPathRequest {
        path: root.join("missing.txt").to_string_lossy().into_owned(),
    })
    .expect_err("missing path");
    assert!(missing_path_error.contains("Failed to open"));
}

#[test]
fn commands_cover_watch_size_volume_and_logging_flows() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let everything_status = commands::everything_status(test_app.app.state::<SizeService>());
    assert!(!everything_status.is_available);
    assert!(matches!(
        everything_status.status,
        EverythingStatusKind::Unavailable
            | EverythingStatusKind::Unsupported
            | EverythingStatusKind::NotReady
    ));

    let size_root = root.join("sizes");
    for index in 0..200 {
        write_file(
            &size_root.join(format!("file-{index}.txt")),
            b"0123456789abcdef0123456789abcdef",
        );
    }

    let derived_path = size_path_from_string(&size_root.to_string_lossy());
    assert_eq!(derived_path, size_root);

    let cancel = commands::cancel_size(
        CancelSizeRequest {
            path: size_root.to_string_lossy().into_owned(),
        },
        test_app.app.state::<SizeService>(),
    );
    assert!(!cancel.cancelled);

    assert_eq!(commands::frontend_log_level("error"), log::Level::Error);
    assert_eq!(commands::frontend_log_level("trace"), log::Level::Debug);
    assert_eq!(
        commands::format_frontend_log(Some("ipc"), "refresh issued", Some("{\"path\":\"left\"}")),
        "[ipc] refresh issued {\"path\":\"left\"}"
    );
    assert_eq!(
        commands::format_frontend_log(None, "refresh issued", None),
        "[frontend] refresh issued"
    );

    commands::log_frontend(LogFrontendRequest {
        level: "trace".to_string(),
        message: "refresh issued".to_string(),
        category: Some("ipc".to_string()),
        details: Some("{\"path\":\"left\"}".to_string()),
    });

    let state = test_app.app.state::<WatchService>();
    let refresh_error = state.refresh_tab(
        file_explorer_lib::watch::WatchTarget {
            tab_id: "left-1".to_string(),
            path: root.join("missing").to_string_lossy().into_owned(),
            sort_key: SortKey::Name,
            sort_direction: SortDirection::Asc,
            filter: String::new(),
            show_hidden: true,
        },
        |_| {},
    );
    assert!(refresh_error.is_err());

    let _ = SizeStateKind::Unknown;
}

#[test]
fn commands_cover_queue_lifecycle_flows() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).expect("dest");

    write_file(&root.join("dup.txt"), b"new-content");
    write_file(&dest.join("dup.txt"), b"old-content");

    let conflict_id = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![op_item(&root.join("dup.txt"))],
        },
        test_app.app.state::<OpsService>(),
    );
    assert_eq!(
        test_app.wait_for_op(&conflict_id, |status| status == OpStatus::Conflict),
        OpStatus::Conflict
    );

    let pending_one_source = root.join("pending-one.txt");
    let pending_two_source = root.join("pending-two.txt");
    write_file(&pending_one_source, b"one");
    write_file(&pending_two_source, b"two");

    let pending_one = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![op_item(&pending_one_source)],
        },
        test_app.app.state::<OpsService>(),
    );
    let pending_two = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![op_item(&pending_two_source)],
        },
        test_app.app.state::<OpsService>(),
    );

    test_app.wait_for_op(&pending_one, |status| status == OpStatus::Pending);
    test_app.wait_for_op(&pending_two, |status| status == OpStatus::Pending);

    commands::reorder_ops(
        ReorderOpsRequest {
            ids: vec![pending_two.clone(), pending_one.clone()],
        },
        test_app.app.state::<OpsService>(),
    );

    let order = commands::queue_snapshot(test_app.app.state::<OpsService>())
        .into_iter()
        .map(|snapshot| snapshot.progress.operation_id)
        .collect::<Vec<_>>();
    let pending_one_index = order.iter().position(|id| id == &pending_one).unwrap();
    let pending_two_index = order.iter().position(|id| id == &pending_two).unwrap();
    assert!(pending_two_index < pending_one_index);

    assert!(commands::has_unfinished_ops(
        test_app.app.state::<OpsService>()
    ));

    commands::cancel_op(
        OpIdRequest {
            id: pending_two.clone(),
        },
        test_app.app.state::<OpsService>(),
    );
    test_app.wait_for_op(&pending_two, |status| status == OpStatus::Cancelled);

    commands::resolve_conflict(
        ResolveConflictRequest {
            id: conflict_id.clone(),
            resolution: ConflictResolution::Replace,
            apply_to_all: false,
            rename_to: None,
        },
        test_app.app.state::<OpsService>(),
    );
    test_app.wait_for_op(&conflict_id, |status| status == OpStatus::Completed);
    assert_eq!(fs::read(dest.join("dup.txt")).expect("dup"), b"new-content");

    test_app.wait_for_op(&pending_one, |status| status == OpStatus::Completed);

    let failed_id = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![OpItem {
                source_path: root.join("missing.txt").to_string_lossy().into_owned(),
                name: "missing.txt".to_string(),
                size_bytes: 1,
            }],
        },
        test_app.app.state::<OpsService>(),
    );
    test_app.wait_for_op(&failed_id, |status| status == OpStatus::Failed);

    commands::retry_op(
        OpIdRequest {
            id: failed_id.clone(),
        },
        test_app.app.state::<OpsService>(),
    );
    test_app.wait_for_op(&failed_id, |status| status == OpStatus::Failed);

    let paused_source = root.join("pause.txt");
    write_file(&paused_source, &vec![b'x'; 8192]);
    let paused_id = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![op_item(&paused_source)],
        },
        test_app.app.state::<OpsService>(),
    );
    commands::pause_op(
        OpIdRequest {
            id: paused_id.clone(),
        },
        test_app.app.state::<OpsService>(),
    );
    test_app.wait_for_op(&paused_id, |status| {
        matches!(status, OpStatus::Paused | OpStatus::Completed)
    });
    commands::resume_op(
        OpIdRequest {
            id: paused_id.clone(),
        },
        test_app.app.state::<OpsService>(),
    );
    test_app.wait_for_op(&paused_id, |status| status == OpStatus::Completed);
}
