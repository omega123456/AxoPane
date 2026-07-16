#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    CancelSizeRequest, CancelSizesRequest, CreateEntryRequest, LogFrontendRequest, OpIdRequest,
    OpenPathRequest, ReorderOpsRequest, ResolveConflictRequest, SaveConfigRequest,
    SaveSessionRequest, SetTabWatchRequest, TrashEntriesRequest,
};
use file_explorer_lib::ops::{
    ConflictResolution, OpItem, OpKind, OpStatus, OpsService, StartOpRequest,
};
use file_explorer_lib::persist::{Config, PersistenceState, Session};
use file_explorer_lib::size::{size_path_from_string, SizeService};
use tempfile::tempdir;

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

fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
    // Safety: tauri::State is a transparent wrapper over an immutable reference
    // and these tests keep the borrowed values alive for the full call.
    unsafe { std::mem::transmute::<&'a T, tauri::State<'a, T>>(value) }
}

fn wait_for_op(service: &OpsService, id: &str, predicate: impl Fn(OpStatus) -> bool) -> OpStatus {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = commands::queue_snapshot(as_state(service))
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

#[test]
fn commands_cover_filesystem_and_persistence_state() {
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");

    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let shell = commands::get_initial_shell();
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let initial_config = commands::load_config(as_state(&persistence));
    assert_eq!(initial_config.theme, "system");

    let saved_config = Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        dismissed_everything_banner: true,
        keybindings: initial_config.keybindings.clone(),
        columns: initial_config.columns.clone(),
        layout: initial_config.layout.clone(),
        update_check_interval: "12h".to_string(),
        log_level: "debug".to_string(),
        date_format: initial_config.date_format.clone(),
        show_time: initial_config.show_time,
        show_seconds: initial_config.show_seconds,
        relative_dates: initial_config.relative_dates,
        auto_folder_size: initial_config.auto_folder_size,
        auto_expand_active_queue_toasts: initial_config.auto_expand_active_queue_toasts,
        favourites: vec![root.join("favourite").to_string_lossy().into_owned()],
    };
    assert_eq!(
        commands::save_config(
            SaveConfigRequest {
                config: saved_config.clone(),
            },
            as_state(&persistence),
        ),
        saved_config
    );
    assert_eq!(commands::load_config(as_state(&persistence)), saved_config);

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
            as_state(&persistence),
        ),
        saved_session
    );
    assert_eq!(
        commands::load_session(as_state(&persistence)),
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
        include_item_counts: true,
    })
    .expect("list dir");
    assert_eq!(listing.entries.len(), 2);

    let renamed = commands::rename_entry(file_explorer_lib::ipc::types::RenameEntryRequest {
        path: root.join("todo.txt").to_string_lossy().into_owned(),
        new_name: "done.txt".to_string(),
    })
    .expect("rename");
    assert_eq!(renamed.name, "done.txt");

    commands::move_to_trash(TrashEntriesRequest {
        paths: vec![
            root.join("Docs").to_string_lossy().into_owned(),
            root.join("done.txt").to_string_lossy().into_owned(),
        ],
    })
    .expect("move to trash");
    assert!(!root.join("Docs").exists());
    assert!(!root.join("done.txt").exists());

    let missing_dir_error = commands::list_dir(file_explorer_lib::ipc::types::ListDirRequest {
        path: root.join("missing").to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: true,
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
fn commands_cover_size_and_logging_state() {
    let size_service = SizeService::default();
    let watch_service = std::sync::Arc::new(file_explorer_lib::watch::WatchService::default());

    let fixture = tempdir().expect("temp dir");
    let size_root = fixture.path().join("sizes");
    for index in 0..50 {
        write_file(
            &size_root.join(format!("file-{index}.txt")),
            b"0123456789abcdef0123456789abcdef",
        );
    }

    let everything_status = commands::everything_status(as_state(&size_service));
    assert!(!everything_status.is_available);
    let listed_volumes = commands::list_volumes().expect("list volumes");
    assert!(!listed_volumes.is_empty());

    let size_batches = commands::request_folder_size(
        file_explorer_lib::ipc::types::FolderSizeRequest {
            path: size_root.to_string_lossy().into_owned(),
        },
        as_state(&size_service),
    );
    let size_events = size_batches.into_iter().flatten().collect::<Vec<_>>();
    assert!(size_events.iter().any(|event| {
        matches!(
            event.state,
            file_explorer_lib::size::SizeStateKind::Ready
                | file_explorer_lib::size::SizeStateKind::Na
        )
    }));

    let cancel = commands::cancel_size(
        CancelSizeRequest {
            path: size_root.to_string_lossy().into_owned(),
        },
        as_state(&size_service),
    );
    assert!(!cancel.cancelled);

    // The batch cancel command reports zero for paths with no in-flight jobs.
    let batch_cancel = commands::cancel_sizes(
        CancelSizesRequest {
            paths: vec![size_root.to_string_lossy().into_owned()],
        },
        as_state(&size_service),
    );
    assert_eq!(batch_cancel.cancelled, 0);

    let network_size_batches = commands::request_folder_sizes(
        file_explorer_lib::ipc::types::FolderSizesRequest {
            paths: vec![
                size_root.to_string_lossy().into_owned(),
                if cfg!(windows) {
                    "Z:\\".to_string()
                } else {
                    "/Volumes/fixture-network".to_string()
                },
            ],
        },
        as_state(&size_service),
    );
    let network_size_events = network_size_batches
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(network_size_events.iter().any(|event| {
        event.path == size_root.to_string_lossy()
            && matches!(
                event.state,
                file_explorer_lib::size::SizeStateKind::Ready
                    | file_explorer_lib::size::SizeStateKind::Na
            )
    }));
    assert!(network_size_events.iter().any(|event| {
        matches!(event.state, file_explorer_lib::size::SizeStateKind::Na)
            && event.source == file_explorer_lib::size::SizeSource::Network
    }));

    assert_eq!(
        size_path_from_string(&size_root.to_string_lossy()),
        size_root
    );

    let icon_paths = vec![
        size_root.join("file-0.txt").to_string_lossy().into_owned(),
        size_root.join("file-1.txt").to_string_lossy().into_owned(),
    ];
    let icon_batches =
        commands::request_icons(file_explorer_lib::ipc::types::RequestIconsRequest {
            paths: icon_paths.clone(),
        });
    let icon_events: Vec<_> = icon_batches.into_iter().flatten().collect();
    assert_eq!(
        icon_events
            .iter()
            .map(|event| event.path.clone())
            .collect::<Vec<_>>(),
        icon_paths
    );
    assert!(icon_events
        .iter()
        .all(|event| event.icon_data_url.is_none()));
    let thumbnail_service = Arc::new(file_explorer_lib::thumbnails::ThumbnailService::new(
        Arc::new(file_explorer_lib::resource_coordinator::ResourceCoordinator::new()),
    ));
    commands::request_thumbnails(
        file_explorer_lib::ipc::types::RequestThumbnailsRequest {
            pane_id: "left".into(),
            tab_id: "tab".into(),
            path: size_root.to_string_lossy().into_owned(),
            generation: 1,
            revision: 1,
            candidates: vec![file_explorer_lib::ipc::types::ThumbnailCandidateRequest {
                path: size_root.join("file-0.txt").to_string_lossy().into_owned(),
                modified_unix_seconds: 0,
                size_bytes: 32,
                is_directory: false,
                priority: file_explorer_lib::ipc::types::ThumbnailPriority::Visible,
                order: 0,
            }],
        },
        as_state(&thumbnail_service),
    )
    .expect("request thumbnails");
    commands::cancel_thumbnails(
        file_explorer_lib::ipc::types::CancelThumbnailsRequest {
            pane_id: "left".into(),
            tab_id: "tab".into(),
            path: size_root.to_string_lossy().into_owned(),
            generation: 1,
        },
        as_state(&thumbnail_service),
    );
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

    let watch_target = file_explorer_lib::watch::WatchTarget {
        tab_id: "left-1".to_string(),
        path: fixture.path().to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };
    commands::set_tab_watch(
        SetTabWatchRequest {
            target: Some(watch_target.clone()),
            entries: None,
        },
        as_state(&watch_service),
    )
    .expect("set tab watch");
    write_file(&fixture.path().join("watch.txt"), b"watch");
    commands::set_tab_watch(
        SetTabWatchRequest {
            target: Some(watch_target.clone()),
            entries: None,
        },
        as_state(&watch_service),
    )
    .expect("reseed tab watch after file write");
    let baseline =
        file_explorer_lib::watch::tab_snapshot_for_tests(&watch_service, &watch_target.tab_id)
            .expect("baseline recorded for tab");
    assert!(baseline.keys().any(|path| path.ends_with("watch.txt")));
    commands::set_tab_watch(
        SetTabWatchRequest {
            target: None,
            entries: None,
        },
        as_state(&watch_service),
    )
    .expect("clear tab watch");

    commands::log_frontend(LogFrontendRequest {
        level: "trace".to_string(),
        message: "refresh issued".to_string(),
        category: Some("ipc".to_string()),
        details: Some("{\"path\":\"left\"}".to_string()),
    });
}

#[test]
fn commands_cover_queue_state_lifecycle() {
    let service = OpsService::new(Duration::from_secs(30));

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
        as_state(&service),
    );
    assert_eq!(
        wait_for_op(&service, &conflict_id, |status| status
            == OpStatus::Conflict),
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
        as_state(&service),
    );
    let pending_two = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![op_item(&pending_two_source)],
        },
        as_state(&service),
    );

    wait_for_op(&service, &pending_one, |status| status == OpStatus::Pending);
    wait_for_op(&service, &pending_two, |status| status == OpStatus::Pending);

    commands::reorder_ops(
        ReorderOpsRequest {
            ids: vec![pending_two.clone(), pending_one.clone()],
        },
        as_state(&service),
    );

    let order = commands::queue_snapshot(as_state(&service))
        .into_iter()
        .map(|snapshot| snapshot.progress.operation_id)
        .collect::<Vec<_>>();
    let pending_one_index = order.iter().position(|id| id == &pending_one).unwrap();
    let pending_two_index = order.iter().position(|id| id == &pending_two).unwrap();
    assert!(pending_two_index < pending_one_index);

    assert!(commands::has_unfinished_ops(as_state(&service)));

    commands::skip_op(
        OpIdRequest {
            id: pending_two.clone(),
        },
        as_state(&service),
    );
    wait_for_op(&service, &pending_two, |status| status == OpStatus::Pending);

    commands::cancel_op(
        OpIdRequest {
            id: pending_two.clone(),
        },
        as_state(&service),
    );
    wait_for_op(&service, &pending_two, |status| {
        status == OpStatus::Cancelled
    });

    commands::resolve_conflict(
        ResolveConflictRequest {
            id: conflict_id.clone(),
            resolution: ConflictResolution::Replace,
            apply_to_all: false,
            rename_to: None,
        },
        as_state(&service),
    );
    wait_for_op(&service, &conflict_id, |status| {
        status == OpStatus::Completed
    });
    assert_eq!(fs::read(dest.join("dup.txt")).expect("dup"), b"new-content");

    wait_for_op(&service, &pending_one, |status| {
        status == OpStatus::Completed
    });

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
        as_state(&service),
    );
    wait_for_op(&service, &failed_id, |status| status == OpStatus::Failed);

    commands::retry_op(
        OpIdRequest {
            id: failed_id.clone(),
        },
        as_state(&service),
    );
    wait_for_op(&service, &failed_id, |status| status == OpStatus::Failed);

    let paused_source = root.join("pause.txt");
    write_file(&paused_source, &vec![b'x'; 8192]);
    let paused_id = commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: dest.to_string_lossy().into_owned(),
            items: vec![op_item(&paused_source)],
        },
        as_state(&service),
    );
    commands::pause_op(
        OpIdRequest {
            id: paused_id.clone(),
        },
        as_state(&service),
    );
    wait_for_op(&service, &paused_id, |status| {
        matches!(status, OpStatus::Paused | OpStatus::Completed)
    });
    commands::resume_op(
        OpIdRequest {
            id: paused_id.clone(),
        },
        as_state(&service),
    );
    wait_for_op(&service, &paused_id, |status| status == OpStatus::Completed);
}
