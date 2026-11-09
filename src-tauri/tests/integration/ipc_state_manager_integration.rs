use crate::common;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    AddIndexExclusionRequest, CancelSizeRequest, CancelSizesRequest, CreateEntryRequest,
    IndexCommandError, LogFrontendRequest, OpIdRequest, OpenPathRequest,
    RemoveIndexExclusionRequest, ReorderOpsRequest, ResolveConflictRequest, SaveConfigRequest,
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

pub(crate) fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
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

        if let Some(status) = status.filter(|status| predicate(*status)) {
            return status;
        }

        assert!(Instant::now() < deadline, "timed out waiting for op {id}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn list_request(path: &Path, show_hidden: bool) -> file_explorer_lib::ipc::types::ListDirRequest {
    file_explorer_lib::ipc::types::ListDirRequest {
        path: path_text(path),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden,
        include_item_counts: true,
    }
}

fn customised_config(initial: &Config, root: &Path) -> Config {
    Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        dismissed_everything_banner: true,
        update_check_interval: "12h".to_string(),
        log_level: "debug".to_string(),
        favourites: vec![path_text(&root.join("favourite"))],
        ..initial.clone()
    }
}

#[test]
fn commands_round_trip_config_through_persistence() {
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");
    let fixture = tempdir().expect("temp dir");

    let shell = commands::get_initial_shell();
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let initial_config = commands::load_config(as_state(&persistence));
    assert_eq!(initial_config.theme, "system");

    let saved_config = customised_config(&initial_config, fixture.path());
    let request = SaveConfigRequest {
        config: saved_config.clone(),
    };
    assert_eq!(
        commands::save_config(
            request,
            as_state(&persistence),
            as_state(&SizeService::default())
        ),
        saved_config
    );
    assert_eq!(commands::load_config(as_state(&persistence)), saved_config);
}

#[test]
fn commands_round_trip_session_through_persistence() {
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let saved_session = Session {
        active_pane: "right".to_string(),
        left_path: path_text(&root.join("left")),
        right_path: path_text(&root.join("right")),
        left: None,
        right: None,
        recent_folders: Vec::new(),
    };
    let request = SaveSessionRequest {
        session: saved_session.clone(),
    };
    assert_eq!(
        commands::save_session(request, as_state(&persistence)),
        saved_session
    );
    assert_eq!(
        commands::load_session(as_state(&persistence)),
        saved_session
    );
}

#[test]
fn commands_cover_filesystem_entry_lifecycle() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let create_request = |name: &str| CreateEntryRequest {
        parent: path_text(root),
        name: name.to_string(),
    };

    let folder = commands::create_folder(create_request("Docs")).expect("create folder");
    assert!(folder.is_dir);
    let file = commands::create_file(create_request("todo.txt")).expect("create file");
    assert_eq!(file.name, "todo.txt");

    let listing = commands::list_dir(list_request(root, true)).expect("list dir");
    assert_eq!(listing.entries.len(), 2);

    let renamed = commands::rename_entry(file_explorer_lib::ipc::types::RenameEntryRequest {
        path: path_text(&root.join("todo.txt")),
        new_name: "done.txt".to_string(),
    })
    .expect("rename");
    assert_eq!(renamed.name, "done.txt");

    commands::move_to_trash(TrashEntriesRequest {
        paths: vec![
            path_text(&root.join("Docs")),
            path_text(&root.join("done.txt")),
        ],
    })
    .expect("move to trash");
    assert!(!root.join("Docs").exists());
    assert!(!root.join("done.txt").exists());
}

#[test]
fn commands_report_missing_paths() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let missing_dir_error = commands::list_dir(list_request(&root.join("missing"), false))
        .expect_err("missing directory");
    assert!(missing_dir_error.contains("Failed to load"));

    let missing_path_error = commands::open_path(OpenPathRequest {
        path: path_text(&root.join("missing.txt")),
    })
    .expect_err("missing path");
    assert!(missing_path_error.contains("Failed to open"));
}

const SIZE_FIXTURE_FILES: usize = 50;

fn size_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let fixture = tempdir().expect("temp dir");
    let size_root = fixture.path().join("sizes");
    for index in 0..SIZE_FIXTURE_FILES {
        write_file(
            &size_root.join(format!("file-{index}.txt")),
            b"0123456789abcdef0123456789abcdef",
        );
    }
    (fixture, size_root)
}

fn has_ready_or_na(state: file_explorer_lib::size::SizeStateKind) -> bool {
    matches!(
        state,
        file_explorer_lib::size::SizeStateKind::Ready | file_explorer_lib::size::SizeStateKind::Na
    )
}

fn expected_auto_size_state() -> file_explorer_lib::size::SizeStateKind {
    if cfg!(target_os = "macos") {
        return file_explorer_lib::size::SizeStateKind::Unknown;
    }
    file_explorer_lib::size::SizeStateKind::Ready
}

fn fixture_network_root() -> String {
    if cfg!(windows) {
        return "Z:\\".to_string();
    }
    "/Volumes/fixture-network".to_string()
}

#[test]
fn commands_report_unavailable_size_index_and_auto_sizing() {
    let size_service = SizeService::default();
    let (_fixture, size_root) = size_fixture();

    let status = commands::folder_size_index_status(as_state(&size_service));
    assert!(!status.is_available);
    let backend = if cfg!(windows) {
        file_explorer_lib::size::IndexBackend::Everything
    } else {
        file_explorer_lib::size::IndexBackend::None
    };
    assert_eq!(status.backend, backend);
    assert_eq!(
        status.state,
        file_explorer_lib::size::IndexState::Unavailable
    );
    commands::set_size_subscriptions(
        file_explorer_lib::ipc::types::SizeSubscriptionsRequest {
            pane_id: "left".to_string(),
            current_path: None,
            folder_paths: Vec::new(),
        },
        as_state(&size_service),
    );

    let auto_batches = commands::request_folder_sizes(
        file_explorer_lib::ipc::types::FolderSizesRequest {
            paths: vec![path_text(&size_root)],
            mode: Some(file_explorer_lib::size::RequestMode::Auto),
        },
        as_state(&size_service),
    );
    let auto_state = auto_batches
        .iter()
        .flatten()
        .map(|event| event.state)
        .next_back();
    assert_eq!(auto_state, Some(expected_auto_size_state()));
    let listed_volumes = commands::list_volumes().expect("list volumes");
    assert!(!listed_volumes.is_empty());
}

#[test]
fn commands_size_a_folder_and_cancel_idle_requests() {
    let size_service = SizeService::default();
    let (_fixture, size_root) = size_fixture();

    let size_batches = commands::request_folder_size(
        file_explorer_lib::ipc::types::FolderSizeRequest {
            path: path_text(&size_root),
            mode: None,
        },
        as_state(&size_service),
    );
    let size_events = size_batches.into_iter().flatten().collect::<Vec<_>>();
    assert!(size_events.iter().any(|event| has_ready_or_na(event.state)));

    let cancel = commands::cancel_size(
        CancelSizeRequest {
            path: path_text(&size_root),
        },
        as_state(&size_service),
    );
    assert!(!cancel.cancelled);

    let batch_cancel = commands::cancel_sizes(
        CancelSizesRequest {
            paths: vec![path_text(&size_root)],
        },
        as_state(&size_service),
    );
    assert_eq!(batch_cancel.cancelled, 0);
    assert_eq!(size_path_from_string(&path_text(&size_root)), size_root);
}

#[test]
fn commands_mark_network_folders_as_not_applicable() {
    let size_service = SizeService::default();
    let (_fixture, size_root) = size_fixture();

    let network_size_batches = commands::request_folder_sizes(
        file_explorer_lib::ipc::types::FolderSizesRequest {
            paths: vec![path_text(&size_root), fixture_network_root()],
            mode: None,
        },
        as_state(&size_service),
    );
    let network_size_events = network_size_batches
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(network_size_events
        .iter()
        .any(|event| event.path == size_root.to_string_lossy() && has_ready_or_na(event.state)));
    assert!(network_size_events.iter().any(|event| {
        matches!(event.state, file_explorer_lib::size::SizeStateKind::Na)
            && event.source == file_explorer_lib::size::SizeSource::Network
    }));
}

#[test]
fn commands_return_placeholder_icons_in_request_order() {
    let (_fixture, size_root) = size_fixture();
    let icon_paths = vec![
        path_text(&size_root.join("file-0.txt")),
        path_text(&size_root.join("file-1.txt")),
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
}

fn thumbnail_candidate(path: &Path) -> file_explorer_lib::ipc::types::ThumbnailCandidateRequest {
    file_explorer_lib::ipc::types::ThumbnailCandidateRequest {
        path: path_text(path),
        modified_unix_seconds: 0,
        size_bytes: 32,
        is_directory: false,
        priority: file_explorer_lib::ipc::types::ThumbnailPriority::Visible,
        order: 0,
    }
}

#[test]
fn commands_request_and_cancel_thumbnails() {
    let (_fixture, size_root) = size_fixture();
    let thumbnail_service = Arc::new(file_explorer_lib::thumbnails::ThumbnailService::new(
        Arc::new(file_explorer_lib::resource_coordinator::ResourceCoordinator::new()),
    ));
    commands::request_thumbnails(
        file_explorer_lib::ipc::types::RequestThumbnailsRequest {
            pane_id: "left".into(),
            tab_id: "tab".into(),
            path: path_text(&size_root),
            generation: 1,
            revision: 1,
            candidates: vec![thumbnail_candidate(&size_root.join("file-0.txt"))],
        },
        as_state(&thumbnail_service),
    )
    .expect("request thumbnails");
    commands::cancel_thumbnails(
        file_explorer_lib::ipc::types::CancelThumbnailsRequest {
            pane_id: "left".into(),
            tab_id: "tab".into(),
            path: path_text(&size_root),
            generation: 1,
        },
        as_state(&thumbnail_service),
    );
}

#[test]
fn commands_format_and_record_frontend_logs() {
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
}

fn set_watch(
    watch_service: &Arc<file_explorer_lib::watch::WatchService>,
    target: Option<file_explorer_lib::watch::WatchTarget>,
) -> Result<(), String> {
    commands::set_tab_watch(
        SetTabWatchRequest {
            target,
            entries: None,
        },
        as_state(watch_service),
    )
}

#[test]
fn commands_reseed_and_clear_tab_watches() {
    let watch_service = Arc::new(file_explorer_lib::watch::WatchService::default());
    let fixture = tempdir().expect("temp dir");
    let watch_target = file_explorer_lib::watch::WatchTarget {
        tab_id: "left-1".to_string(),
        path: path_text(fixture.path()),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };
    set_watch(&watch_service, Some(watch_target.clone())).expect("set tab watch");
    write_file(&fixture.path().join("watch.txt"), b"watch");
    set_watch(&watch_service, Some(watch_target.clone()))
        .expect("reseed tab watch after file write");
    let baseline =
        file_explorer_lib::watch::tab_snapshot_for_tests(&watch_service, &watch_target.tab_id)
            .expect("baseline recorded for tab");
    assert!(baseline.keys().any(|path| path.ends_with("watch.txt")));
    set_watch(&watch_service, None).expect("clear tab watch");
}

fn start_copy(service: &OpsService, destination: &Path, item: OpItem) -> String {
    commands::start_op(
        StartOpRequest {
            kind: OpKind::Copy,
            destination_dir: path_text(destination),
            items: vec![item],
        },
        as_state(service),
    )
}

fn op_request(id: &str) -> OpIdRequest {
    OpIdRequest { id: id.to_string() }
}

fn wait_for_status(service: &OpsService, id: &str, expected: OpStatus) -> OpStatus {
    wait_for_op(service, id, |status| status == expected)
}

fn queue_position(service: &OpsService, id: &str) -> usize {
    commands::queue_snapshot(as_state(service))
        .into_iter()
        .position(|snapshot| snapshot.progress.operation_id == id)
        .expect("operation queued")
}

fn missing_item(root: &Path) -> OpItem {
    OpItem {
        source_path: path_text(&root.join("missing.txt")),
        name: "missing.txt".to_string(),
        size_bytes: 1,
    }
}

struct QueueFixture {
    _fixture: tempfile::TempDir,
    root: std::path::PathBuf,
    destination: std::path::PathBuf,
}

fn queue_fixture() -> QueueFixture {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path().to_path_buf();
    let destination = root.join("dest");
    fs::create_dir_all(&destination).expect("dest");
    QueueFixture {
        _fixture: fixture,
        root,
        destination,
    }
}

fn start_blocking_conflict(service: &OpsService, queue: &QueueFixture) -> String {
    write_file(&queue.root.join("dup.txt"), b"new-content");
    write_file(&queue.destination.join("dup.txt"), b"old-content");
    let conflict_id = start_copy(
        service,
        &queue.destination,
        op_item(&queue.root.join("dup.txt")),
    );
    assert_eq!(
        wait_for_status(service, &conflict_id, OpStatus::Conflict),
        OpStatus::Conflict
    );
    conflict_id
}

fn start_pending_copy(service: &OpsService, queue: &QueueFixture, name: &str) -> String {
    let source = queue.root.join(name);
    write_file(&source, name.as_bytes());
    let id = start_copy(service, &queue.destination, op_item(&source));
    wait_for_status(service, &id, OpStatus::Pending);
    id
}

#[test]
fn commands_cover_queue_state_lifecycle() {
    let service = OpsService::new(Duration::from_secs(30));
    let queue = queue_fixture();
    let conflict_id = start_blocking_conflict(&service, &queue);
    let pending_one = start_pending_copy(&service, &queue, "pending-one.txt");
    let pending_two = start_pending_copy(&service, &queue, "pending-two.txt");

    commands::reorder_ops(
        ReorderOpsRequest {
            ids: vec![pending_two.clone(), pending_one.clone()],
        },
        as_state(&service),
    );
    assert!(queue_position(&service, &pending_two) < queue_position(&service, &pending_one));
    assert!(commands::has_unfinished_ops(as_state(&service)));

    commands::skip_op(op_request(&pending_two), as_state(&service));
    wait_for_status(&service, &pending_two, OpStatus::Pending);
    commands::cancel_op(op_request(&pending_two), as_state(&service));
    wait_for_status(&service, &pending_two, OpStatus::Cancelled);

    commands::resolve_conflict(
        ResolveConflictRequest {
            id: conflict_id.clone(),
            resolution: ConflictResolution::Replace,
            apply_to_all: false,
            rename_to: None,
        },
        as_state(&service),
    );
    wait_for_status(&service, &conflict_id, OpStatus::Completed);
    assert_eq!(
        fs::read(queue.destination.join("dup.txt")).expect("dup"),
        b"new-content"
    );
    wait_for_status(&service, &pending_one, OpStatus::Completed);
}

#[test]
fn commands_retry_failed_and_resume_paused_operations() {
    let service = OpsService::new(Duration::from_secs(30));
    let queue = queue_fixture();

    let failed_id = start_copy(&service, &queue.destination, missing_item(&queue.root));
    wait_for_status(&service, &failed_id, OpStatus::Failed);
    commands::retry_op(op_request(&failed_id), as_state(&service));
    wait_for_status(&service, &failed_id, OpStatus::Failed);

    let paused_source = queue.root.join("pause.txt");
    write_file(&paused_source, &vec![b'x'; 8192]);
    let paused_id = start_copy(&service, &queue.destination, op_item(&paused_source));
    commands::pause_op(op_request(&paused_id), as_state(&service));
    wait_for_op(&service, &paused_id, |status| {
        matches!(status, OpStatus::Paused | OpStatus::Completed)
    });
    commands::resume_op(op_request(&paused_id), as_state(&service));
    wait_for_status(&service, &paused_id, OpStatus::Completed);
}

#[test]
fn save_config_keeps_the_rust_held_index_exclusions() {
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");
    let held = vec!["/Users/alex/Projects/app/build-cache".to_string()];
    persistence.config.replace(Config {
        index_exclusions: held.clone(),
        ..persistence.config.current()
    });
    let stale = Config {
        theme: "dark".to_string(),
        index_exclusions: Vec::new(),
        ..Config::default()
    };

    let saved = commands::save_config(
        SaveConfigRequest { config: stale },
        as_state(&persistence),
        as_state(&SizeService::default()),
    );

    assert_eq!(saved.index_exclusions, held);
    assert_eq!(saved.theme, "dark");
    assert_eq!(persistence.config.current().index_exclusions, held);
}

#[test]
fn index_exclusion_commands_are_unsupported_without_a_macos_index() {
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");
    let size_service = SizeService::default();

    let added = commands::add_index_exclusion(
        AddIndexExclusionRequest {
            paths: vec![path_text(config_dir.path())],
        },
        as_state(&size_service),
        as_state(&persistence),
    );
    let removed = commands::remove_index_exclusion(
        RemoveIndexExclusionRequest {
            path: path_text(config_dir.path()),
        },
        as_state(&size_service),
        as_state(&persistence),
    );

    assert_eq!(added.map(|_| ()), Err(IndexCommandError::Unsupported));
    assert_eq!(removed.map(|_| ()), Err(IndexCommandError::Unsupported));
    assert_eq!(
        commands::pick_index_exclusions(as_state(&size_service)).map(|_| ()),
        Err(IndexCommandError::Unsupported)
    );
    assert_eq!(
        commands::retry_folder_size_index(as_state(&size_service)),
        Err(IndexCommandError::Unsupported)
    );
    assert_eq!(
        commands::rebuild_folder_size_index(as_state(&size_service)),
        Err(IndexCommandError::Unsupported)
    );
    assert_eq!(
        commands::folder_size_index_storage(as_state(&size_service)),
        Err(IndexCommandError::Unsupported)
    );
    assert!(persistence.config.current().index_exclusions.is_empty());
}
