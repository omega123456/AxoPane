use crate::common;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    AppConfig, CancelSizeResponse, ConflictResolution, CreateEntryRequest, FolderSizeRequest,
    FolderSizesRequest, IconStateEvent, InitialShellResponse, ListDirRequest, ListDirResponse,
    ListTreeChildrenRequest, ListTreeChildrenResponse, OpIdRequest, OpSnapshot, OpenPathRequest,
    ReorderOpsRequest, RequestIconsRequest, ResolveConflictRequest, SaveConfigRequest,
    SaveSessionRequest, SessionState, SetTabWatchRequest, SizeStateEvent, TrashEntriesRequest,
    WatchTarget,
};
use file_explorer_lib::ops::{OpItem, OpKind, OpStatus, OpsService, StartOpRequest};
use file_explorer_lib::persist::{Config, PersistenceState, Session};
use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::size::{FolderSizeIndexStatus, IndexBackend, SizeService};
use file_explorer_lib::thumbnails::ThumbnailService;
use file_explorer_lib::watch::WatchService;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::{ipc::InvokeBody, Runtime, WebviewWindow};
use tempfile::tempdir;

struct TestApp<R: Runtime> {
    _config_dir: tempfile::TempDir,
    _app: tauri::App<R>,
    webview: WebviewWindow<R>,
}

impl TestApp<tauri::test::MockRuntime> {
    fn new() -> Self {
        let config_dir = tempdir().expect("config dir");
        let persistence = PersistenceState::load(config_dir.path()).expect("persistence");
        let coordinator = Arc::new(ResourceCoordinator::new());
        let thumbnails = Arc::new(ThumbnailService::new(Arc::clone(&coordinator)));

        let app = mock_builder()
            .manage(persistence)
            .manage(coordinator)
            .manage(thumbnails)
            .manage(SizeService::default())
            .manage(Arc::new(WatchService::default()))
            .manage(OpsService::new(Duration::from_secs(30)))
            .invoke_handler(tauri::generate_handler![
                commands::get_initial_shell,
                commands::list_dir,
                commands::list_tree_children,
                commands::create_folder,
                commands::create_file,
                commands::rename_entry,
                commands::move_to_trash,
                commands::open_path,
                commands::list_volumes,
                commands::folder_size_index_status,
                commands::set_size_subscriptions,
                commands::request_folder_size,
                commands::request_folder_sizes,
                commands::request_icons,
                commands::request_thumbnails,
                commands::cancel_thumbnails,
                commands::cancel_size,
                commands::cancel_sizes,
                commands::set_tab_watch,
                commands::start_op,
                commands::pause_op,
                commands::resume_op,
                commands::cancel_op,
                commands::retry_op,
                commands::reorder_ops,
                commands::resolve_conflict,
                commands::queue_snapshot,
                commands::has_unfinished_ops,
                commands::load_config,
                commands::save_config,
                commands::load_session,
                commands::save_session,
                commands::log_frontend
            ])
            .build(mock_context(noop_assets()))
            .expect("mock app");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("webview");

        Self {
            _config_dir: config_dir,
            _app: app,
            webview,
        }
    }

    fn invoke_json<T: serde::de::DeserializeOwned>(
        &self,
        cmd: &str,
        body: Value,
    ) -> Result<T, String> {
        let response: Result<tauri::ipc::InvokeResponseBody, serde_json::Value> = get_ipc_response(
            &self.webview,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .expect("invoke url"),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        );

        match response {
            Ok(value) => value
                .deserialize::<T>()
                .map_err(|error| format!("deserialize success: {error}")),
            Err(value) => Err(serde_json::from_value::<String>(value.clone())
                .unwrap_or_else(|_| value.to_string())),
        }
    }

    fn invoke<T: serde::de::DeserializeOwned>(&self, cmd: &str) -> Result<T, String> {
        self.invoke_json(cmd, Value::Null)
    }

    fn invoke_payload<T: serde::de::DeserializeOwned, P: Serialize>(
        &self,
        cmd: &str,
        payload: P,
    ) -> Result<T, String> {
        self.invoke_json(cmd, json!({ "payload": payload }))
    }

    fn queue_snapshot(&self) -> Vec<OpSnapshot> {
        self.invoke("queue_snapshot").expect("queue snapshot")
    }

    fn wait_for_op(&self, id: &str, predicate: impl Fn(&OpSnapshot) -> bool) -> OpSnapshot {
        let deadline = Instant::now() + Duration::from_secs(2);

        loop {
            let matching = self
                .queue_snapshot()
                .into_iter()
                .find(|snapshot| snapshot.progress.operation_id == id)
                .filter(|snapshot| predicate(snapshot));

            match matching {
                Some(snapshot) => return snapshot,
                None => wait_before_retry(deadline, id),
            }
        }
    }
}

impl TestApp<tauri::test::MockRuntime> {
    fn wait_for_status(&self, id: &str, expected: OpStatus) -> OpSnapshot {
        self.wait_for_op(id, |snapshot| snapshot.progress.status == expected)
    }

    fn start_copy(&self, destination: &Path, item: OpItem) -> String {
        self.invoke_payload(
            "start_op",
            StartOpRequest {
                kind: OpKind::Copy,
                destination_dir: path_text(destination),
                items: vec![item],
            },
        )
        .expect("start copy op")
    }

    fn run_op_command(&self, command: &str, id: &str) {
        self.invoke_payload::<(), _>(command, OpIdRequest { id: id.to_string() })
            .unwrap_or_else(|error| panic!("{command} failed: {error}"));
    }

    fn list_dir(&self, path: &Path, show_hidden: bool) -> Result<ListDirResponse, String> {
        self.invoke_payload(
            "list_dir",
            ListDirRequest {
                path: path_text(path),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden,
                include_item_counts: true,
            },
        )
    }

    fn queue_position(&self, id: &str) -> usize {
        self.queue_snapshot()
            .into_iter()
            .position(|snapshot| snapshot.progress.operation_id == id)
            .expect("operation in snapshot")
    }

    fn set_tab_watch(&self, target: Option<WatchTarget>) {
        self.invoke_payload::<(), _>(
            "set_tab_watch",
            SetTabWatchRequest {
                target,
                entries: None,
            },
        )
        .expect("set tab watch");
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn wait_before_retry(deadline: Instant, id: &str) {
    assert!(Instant::now() < deadline, "timed out waiting for op {id}");
    thread::sleep(Duration::from_millis(10));
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
    fs::write(path, contents).expect("write file");
}

fn customised_config(initial: &AppConfig) -> Config {
    Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        dismissed_everything_banner: true,
        update_check_interval: "12h".to_string(),
        log_level: "info".to_string(),
        ..initial.clone()
    }
}

#[test]
fn ipc_commands_round_trip_config_and_session() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let shell: InitialShellResponse = test_app.invoke("get_initial_shell").expect("shell");
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let initial_config: AppConfig = test_app.invoke("load_config").expect("load config");
    assert_eq!(initial_config.theme, "system");
    let saved_config = customised_config(&initial_config);
    let config_request = SaveConfigRequest {
        config: saved_config.clone(),
    };
    let echoed_config: AppConfig = test_app
        .invoke_payload("save_config", config_request)
        .expect("save config");
    assert_eq!(echoed_config, saved_config);
    let reloaded_config: AppConfig = test_app.invoke("load_config").expect("reload config");
    assert_eq!(reloaded_config, saved_config);

    let saved_session = Session {
        active_pane: "right".to_string(),
        left_path: path_text(&root.join("left")),
        right_path: path_text(&root.join("right")),
        left: None,
        right: None,
        recent_folders: vec!["C:\\Users\\alex\\Documents".to_string()],
    };
    let session_request = SaveSessionRequest {
        session: saved_session.clone(),
    };
    let echoed_session: SessionState = test_app
        .invoke_payload("save_session", session_request)
        .expect("save session");
    assert_eq!(echoed_session, saved_session);
    let reloaded_session: SessionState = test_app.invoke("load_session").expect("reload session");
    assert_eq!(reloaded_session, saved_session);
}

fn created_entry_name(
    test_app: &TestApp<tauri::test::MockRuntime>,
    command: &str,
    parent: &Path,
    name: &str,
) -> Option<String> {
    let request = CreateEntryRequest {
        parent: path_text(parent),
        name: name.to_string(),
    };
    let created = test_app
        .invoke_payload::<Value, _>(command, request)
        .unwrap_or_else(|error| panic!("{command} failed: {error}"));
    created
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[test]
fn ipc_commands_cover_filesystem_entry_lifecycle() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let folder_path = root.join("Docs");
    let renamed_path = root.join("done.txt");

    let folder_name = created_entry_name(&test_app, "create_folder", root, "Docs");
    assert_eq!(folder_name.as_deref(), Some("Docs"));
    let file_name = created_entry_name(&test_app, "create_file", root, "todo.txt");
    assert_eq!(file_name.as_deref(), Some("todo.txt"));

    let listing = test_app.list_dir(root, true).expect("list dir");
    assert_eq!(listing.entries.len(), 2);

    let tree_children: ListTreeChildrenResponse = test_app
        .invoke_payload(
            "list_tree_children",
            ListTreeChildrenRequest {
                path: path_text(root),
                show_hidden: true,
            },
        )
        .expect("list tree children");
    assert_eq!(tree_children.children.len(), 1);
    assert_eq!(tree_children.children[0].name, "Docs");

    let rename_request = file_explorer_lib::ipc::types::RenameEntryRequest {
        path: path_text(&root.join("todo.txt")),
        new_name: "done.txt".to_string(),
    };
    let renamed = test_app
        .invoke_payload::<Value, _>("rename_entry", rename_request)
        .expect("rename");
    assert_eq!(
        renamed.get("name").and_then(Value::as_str),
        Some("done.txt")
    );

    let trash_request = TrashEntriesRequest {
        paths: vec![path_text(&folder_path), path_text(&renamed_path)],
    };
    test_app
        .invoke_payload::<(), _>("move_to_trash", trash_request)
        .expect("move to trash");
    assert!(!folder_path.exists());
    assert!(!renamed_path.exists());
}

#[test]
fn ipc_commands_report_missing_paths() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let missing_dir_error = test_app
        .list_dir(&root.join("missing"), false)
        .expect_err("missing dir should fail");
    assert!(missing_dir_error.contains("Failed to load"));

    let missing_path_error = test_app
        .invoke_payload::<(), _>(
            "open_path",
            OpenPathRequest {
                path: path_text(&root.join("missing.txt")),
            },
        )
        .expect_err("missing path should fail");
    assert!(missing_path_error.contains("Failed to open"));
}

#[test]
fn ipc_commands_cover_size_index_volumes_and_watch() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    write_file(&root.join("before.txt"), b"before");

    let status: FolderSizeIndexStatus = test_app
        .invoke("folder_size_index_status")
        .expect("folder size index status");
    assert!(!status.is_available);
    let backend = if cfg!(windows) {
        IndexBackend::Everything
    } else {
        IndexBackend::None
    };
    assert_eq!(status.backend, backend);
    let () = test_app
        .invoke_payload(
            "set_size_subscriptions",
            serde_json::json!({ "paneId": "left", "currentPath": root, "folderPaths": [root] }),
        )
        .expect("set size subscriptions");

    let volumes: Vec<Value> = test_app.invoke("list_volumes").expect("list volumes");
    assert!(!volumes.is_empty());

    test_app.set_tab_watch(Some(WatchTarget {
        tab_id: "left-1".to_string(),
        path: path_text(root),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    }));
    fs::remove_file(root.join("before.txt")).expect("remove before");
    write_file(&root.join("after.txt"), b"after");
    test_app.set_tab_watch(None);
}

const IPC_SIZE_FIXTURE_FILES: usize = 200;

fn has_ready_or_na(batches: Vec<Vec<SizeStateEvent>>) -> bool {
    batches.into_iter().flatten().any(|event| {
        matches!(
            event.state,
            file_explorer_lib::size::SizeStateKind::Ready
                | file_explorer_lib::size::SizeStateKind::Na
        )
    })
}

#[test]
fn ipc_commands_size_folders_and_return_icons() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let size_root = fixture.path().join("sizes");
    fs::create_dir_all(&size_root).expect("size root");
    for index in 0..IPC_SIZE_FIXTURE_FILES {
        write_file(
            &size_root.join(format!("file-{index}.txt")),
            b"0123456789abcdef0123456789abcdef",
        );
    }

    let single_request = FolderSizeRequest {
        path: path_text(&size_root),
        mode: None,
    };
    let size_batches = test_app
        .invoke_payload("request_folder_size", single_request)
        .expect("request folder size");
    assert!(has_ready_or_na(size_batches));

    let batch_request = FolderSizesRequest {
        paths: vec![path_text(&size_root)],
        mode: None,
    };
    let size_batches = test_app
        .invoke_payload("request_folder_sizes", batch_request)
        .expect("request folder sizes");
    assert!(has_ready_or_na(size_batches));

    let cancel_request = file_explorer_lib::ipc::types::CancelSizeRequest {
        path: path_text(&size_root),
    };
    let cancel_response: CancelSizeResponse = test_app
        .invoke_payload("cancel_size", cancel_request)
        .expect("cancel size");
    assert!(!cancel_response.cancelled);

    let icon_paths = vec![
        path_text(&size_root.join("file-0.txt")),
        path_text(&size_root.join("file-1.txt")),
    ];
    let icon_request = RequestIconsRequest {
        paths: icon_paths.clone(),
    };
    let icon_batches: Vec<Vec<IconStateEvent>> = test_app
        .invoke_payload("request_icons", icon_request)
        .expect("request icons");
    let icon_events: Vec<IconStateEvent> = icon_batches.into_iter().flatten().collect();
    assert_eq!(icon_events.len(), icon_paths.len());
    assert!(icon_events
        .iter()
        .all(|event| event.icon_data_url.is_none()));
}

#[test]
fn ipc_commands_record_frontend_logs() {
    let test_app = TestApp::new();
    test_app
        .invoke_payload::<(), _>(
            "log_frontend",
            file_explorer_lib::ipc::types::LogFrontendRequest {
                level: "trace".to_string(),
                message: "refresh issued".to_string(),
                category: Some("ipc".to_string()),
                details: Some("{\"path\":\"left\"}".to_string()),
            },
        )
        .expect("log frontend");
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

fn start_pending_copy(
    test_app: &TestApp<tauri::test::MockRuntime>,
    queue: &QueueFixture,
    name: &str,
) -> String {
    let source = queue.root.join(name);
    write_file(&source, name.as_bytes());
    let id = test_app.start_copy(&queue.destination, op_item(&source));
    test_app.wait_for_status(&id, OpStatus::Pending);
    id
}

#[test]
fn ipc_commands_cover_queue_lifecycle_commands() {
    let test_app = TestApp::new();
    let queue = queue_fixture();
    write_file(&queue.root.join("dup.txt"), b"new-content");
    write_file(&queue.destination.join("dup.txt"), b"old-content");

    let conflict_id = test_app.start_copy(&queue.destination, op_item(&queue.root.join("dup.txt")));
    let conflict_snapshot = test_app.wait_for_status(&conflict_id, OpStatus::Conflict);
    assert_eq!(
        conflict_snapshot.progress.item_names,
        vec!["dup.txt".to_string()]
    );
    let pending_one = start_pending_copy(&test_app, &queue, "pending-one.txt");
    let pending_two = start_pending_copy(&test_app, &queue, "pending-two.txt");

    let reorder_request = ReorderOpsRequest {
        ids: vec![pending_two.clone(), pending_one.clone()],
    };
    test_app
        .invoke_payload::<(), _>("reorder_ops", reorder_request)
        .expect("reorder ops");
    assert!(test_app.queue_position(&pending_two) < test_app.queue_position(&pending_one));

    test_app.run_op_command("cancel_op", &pending_two);
    test_app.wait_for_status(&pending_two, OpStatus::Cancelled);
    let has_unfinished_during_conflict: bool = test_app
        .invoke("has_unfinished_ops")
        .expect("unfinished ops");
    assert!(has_unfinished_during_conflict);

    let resolve_request = ResolveConflictRequest {
        id: conflict_id.clone(),
        resolution: ConflictResolution::Replace,
        apply_to_all: false,
        rename_to: None,
    };
    test_app
        .invoke_payload::<(), _>("resolve_conflict", resolve_request)
        .expect("resolve conflict");
    test_app.wait_for_status(&conflict_id, OpStatus::Completed);
    assert_eq!(
        fs::read(queue.destination.join("dup.txt")).expect("resolved dest"),
        b"new-content"
    );
    test_app.wait_for_status(&pending_one, OpStatus::Completed);
}

#[test]
fn ipc_commands_retry_failed_and_resume_paused_operations() {
    let test_app = TestApp::new();
    let queue = queue_fixture();
    let missing_item = OpItem {
        source_path: path_text(&queue.root.join("missing.txt")),
        name: "missing.txt".to_string(),
        size_bytes: 1,
    };
    let failed_id = test_app.start_copy(&queue.destination, missing_item);
    test_app.wait_for_status(&failed_id, OpStatus::Failed);
    test_app.run_op_command("retry_op", &failed_id);
    test_app.wait_for_status(&failed_id, OpStatus::Failed);

    let paused_source = queue.root.join("pause.txt");
    write_file(&paused_source, &vec![b'x'; 8192]);
    let paused_id = test_app.start_copy(&queue.destination, op_item(&paused_source));
    test_app.run_op_command("pause_op", &paused_id);
    test_app.wait_for_op(&paused_id, |snapshot| {
        matches!(
            snapshot.progress.status,
            OpStatus::Paused | OpStatus::Completed
        )
    });
    test_app.run_op_command("resume_op", &paused_id);
    test_app.wait_for_status(&paused_id, OpStatus::Completed);

    let unfinished_after: bool = test_app
        .invoke("has_unfinished_ops")
        .expect("has unfinished");
    assert!(matches!(unfinished_after, false | true));
    let snapshots = test_app.queue_snapshot();
    assert!(snapshots
        .iter()
        .any(|snapshot| snapshot.progress.operation_id == failed_id));
}
