#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    AppConfig, CancelSizeResponse, ConflictResolution, CreateEntryRequest, FolderSizeRequest,
    FolderSizesRequest, InitialShellResponse, ListDirRequest, ListDirResponse,
    ListTreeChildrenRequest, ListTreeChildrenResponse, OpIdRequest, OpSnapshot, OpenPathRequest,
    RefreshTabRequest, ReorderOpsRequest, ResolveConflictRequest, SaveConfigRequest,
    SaveSessionRequest, SessionState, SetTabWatchRequest, TrashEntriesRequest, WatchDirPatch,
    WatchTarget,
};
use file_explorer_lib::ops::{OpItem, OpKind, OpStatus, OpsService, StartOpRequest};
use file_explorer_lib::persist::{Config, PersistenceState, Session};
use file_explorer_lib::size::{EverythingStatus, SizeService};
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

        let app = mock_builder()
            .manage(persistence)
            .manage(SizeService::default())
            .manage(WatchService::default())
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
                commands::everything_status,
                commands::request_folder_size,
                commands::request_folder_sizes,
                commands::cancel_size,
                commands::set_tab_watch,
                commands::refresh_tab,
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
            let snapshot = self
                .queue_snapshot()
                .into_iter()
                .find(|snapshot| snapshot.progress.operation_id == id);

            if let Some(snapshot) = snapshot {
                if predicate(&snapshot) {
                    return snapshot;
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
    fs::write(path, contents).expect("write file");
}

#[test]
fn ipc_commands_cover_shell_filesystem_and_persistence_flows() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    let shell: InitialShellResponse = test_app.invoke("get_initial_shell").expect("shell");
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(common::bootstrap_message(), "phase-1-common");

    let initial_config: AppConfig = test_app.invoke("load_config").expect("load config");
    assert_eq!(initial_config.theme, "system");

    let saved_config = Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        dismissed_everything_banner: true,
        keybindings: initial_config.keybindings.clone(),
        columns: initial_config.columns.clone(),
        layout: initial_config.layout.clone(),
        update_check_interval: "12h".to_string(),
        log_level: "info".to_string(),
        date_format: initial_config.date_format.clone(),
        show_time: initial_config.show_time,
        show_seconds: initial_config.show_seconds,
        relative_dates: initial_config.relative_dates,
    };
    let echoed_config: AppConfig = test_app
        .invoke_payload(
            "save_config",
            SaveConfigRequest {
                config: saved_config.clone(),
            },
        )
        .expect("save config");
    assert_eq!(echoed_config, saved_config);
    let reloaded_config: AppConfig = test_app.invoke("load_config").expect("reload config");
    assert_eq!(reloaded_config, saved_config);

    let saved_session = Session {
        active_pane: "right".to_string(),
        left_path: root.join("left").to_string_lossy().into_owned(),
        right_path: root.join("right").to_string_lossy().into_owned(),
        left: None,
        right: None,
    };
    let echoed_session: SessionState = test_app
        .invoke_payload(
            "save_session",
            SaveSessionRequest {
                session: saved_session.clone(),
            },
        )
        .expect("save session");
    assert_eq!(echoed_session, saved_session);
    let reloaded_session: SessionState = test_app.invoke("load_session").expect("reload session");
    assert_eq!(reloaded_session, saved_session);

    let folder_path = root.join("Docs");
    let file_path = root.join("todo.txt");

    let folder = test_app
        .invoke_payload::<Value, _>(
            "create_folder",
            CreateEntryRequest {
                parent: root.to_string_lossy().into_owned(),
                name: "Docs".to_string(),
            },
        )
        .expect("create folder");
    assert_eq!(folder.get("name").and_then(Value::as_str), Some("Docs"));

    let file = test_app
        .invoke_payload::<Value, _>(
            "create_file",
            CreateEntryRequest {
                parent: root.to_string_lossy().into_owned(),
                name: "todo.txt".to_string(),
            },
        )
        .expect("create file");
    assert_eq!(file.get("name").and_then(Value::as_str), Some("todo.txt"));

    let listing: ListDirResponse = test_app
        .invoke_payload(
            "list_dir",
            ListDirRequest {
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
            },
        )
        .expect("list dir");
    assert_eq!(listing.entries.len(), 2);

    let tree_children: ListTreeChildrenResponse = test_app
        .invoke_payload(
            "list_tree_children",
            ListTreeChildrenRequest {
                path: root.to_string_lossy().into_owned(),
                show_hidden: true,
            },
        )
        .expect("list tree children");
    assert_eq!(tree_children.children.len(), 1);
    assert_eq!(tree_children.children[0].name, "Docs");

    let renamed = test_app
        .invoke_payload::<Value, _>(
            "rename_entry",
            file_explorer_lib::ipc::types::RenameEntryRequest {
                path: file_path.to_string_lossy().into_owned(),
                new_name: "done.txt".to_string(),
            },
        )
        .expect("rename");
    let renamed_path = root.join("done.txt");
    assert_eq!(
        renamed.get("name").and_then(Value::as_str),
        Some("done.txt")
    );

    test_app
        .invoke_payload::<(), _>(
            "move_to_trash",
            TrashEntriesRequest {
                paths: vec![
                    folder_path.to_string_lossy().into_owned(),
                    renamed_path.to_string_lossy().into_owned(),
                ],
            },
        )
        .expect("move to trash");
    assert!(!folder_path.exists());
    assert!(!renamed_path.exists());

    let missing_dir_error = test_app
        .invoke_payload::<ListDirResponse, _>(
            "list_dir",
            ListDirRequest {
                path: root.join("missing").to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: false,
            },
        )
        .expect_err("missing dir should fail");
    assert!(missing_dir_error.contains("Failed to load"));

    let missing_path_error = test_app
        .invoke_payload::<(), _>(
            "open_path",
            OpenPathRequest {
                path: root.join("missing.txt").to_string_lossy().into_owned(),
            },
        )
        .expect_err("missing path should fail");
    assert!(missing_path_error.contains("Failed to open"));
}

#[test]
fn ipc_commands_cover_watch_size_and_logging_state() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    write_file(&root.join("before.txt"), b"before");

    let everything_status: EverythingStatus = test_app
        .invoke("everything_status")
        .expect("everything status");
    assert!(!everything_status.is_available);

    let volumes: Vec<Value> = test_app.invoke("list_volumes").expect("list volumes");
    assert!(!volumes.is_empty());

    let watch_target = WatchTarget {
        tab_id: "left-1".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    };
    test_app
        .invoke_payload::<(), _>(
            "set_tab_watch",
            SetTabWatchRequest {
                target: Some(watch_target.clone()),
            },
        )
        .expect("set watch");

    fs::remove_file(root.join("before.txt")).expect("remove before");
    write_file(&root.join("after.txt"), b"after");

    let patch: Value = test_app
        .invoke_payload(
            "refresh_tab",
            RefreshTabRequest {
                target: watch_target.clone(),
            },
        )
        .expect("refresh tab");
    let watch_patch: WatchDirPatch =
        serde_json::from_value(patch).expect("deserialize refresh patch");
    assert_eq!(watch_patch.tab_id, "left-1");
    assert!(watch_patch
        .removed
        .iter()
        .any(|path| path.ends_with("before.txt")));
    assert!(watch_patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("after.txt")));

    test_app
        .invoke_payload::<(), _>("set_tab_watch", SetTabWatchRequest { target: None })
        .expect("clear watch");

    let size_root = root.join("sizes");
    fs::create_dir_all(&size_root).expect("size root");
    for index in 0..200 {
        write_file(
            &size_root.join(format!("file-{index}.txt")),
            b"0123456789abcdef0123456789abcdef",
        );
    }

    test_app
        .invoke_payload::<(), _>(
            "request_folder_size",
            FolderSizeRequest {
                path: size_root.to_string_lossy().into_owned(),
            },
        )
        .expect("request folder size");
    test_app
        .invoke_payload::<(), _>(
            "request_folder_sizes",
            FolderSizesRequest {
                paths: vec![size_root.to_string_lossy().into_owned()],
            },
        )
        .expect("request folder sizes");

    let cancel_response: CancelSizeResponse = test_app
        .invoke_payload(
            "cancel_size",
            file_explorer_lib::ipc::types::CancelSizeRequest {
                path: size_root.to_string_lossy().into_owned(),
            },
        )
        .expect("cancel size");
    assert!(cancel_response.cancelled);

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

#[test]
fn ipc_commands_cover_queue_lifecycle_commands() {
    let test_app = TestApp::new();
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).expect("dest");

    write_file(&root.join("dup.txt"), b"new-content");
    write_file(&dest.join("dup.txt"), b"old-content");

    let conflict_id: String = test_app
        .invoke_payload(
            "start_op",
            StartOpRequest {
                kind: OpKind::Copy,
                destination_dir: dest.to_string_lossy().into_owned(),
                items: vec![op_item(&root.join("dup.txt"))],
            },
        )
        .expect("start conflict op");
    let conflict_snapshot = test_app.wait_for_op(&conflict_id, |snapshot| {
        snapshot.progress.status == OpStatus::Conflict
    });
    assert_eq!(
        conflict_snapshot.progress.item_names,
        vec!["dup.txt".to_string()]
    );

    let pending_one_source = root.join("pending-one.txt");
    let pending_two_source = root.join("pending-two.txt");
    write_file(&pending_one_source, b"one");
    write_file(&pending_two_source, b"two");

    let pending_one: String = test_app
        .invoke_payload(
            "start_op",
            StartOpRequest {
                kind: OpKind::Copy,
                destination_dir: dest.to_string_lossy().into_owned(),
                items: vec![op_item(&pending_one_source)],
            },
        )
        .expect("start pending one");
    let pending_two: String = test_app
        .invoke_payload(
            "start_op",
            StartOpRequest {
                kind: OpKind::Copy,
                destination_dir: dest.to_string_lossy().into_owned(),
                items: vec![op_item(&pending_two_source)],
            },
        )
        .expect("start pending two");

    test_app.wait_for_op(&pending_one, |snapshot| {
        snapshot.progress.status == OpStatus::Pending
    });
    test_app.wait_for_op(&pending_two, |snapshot| {
        snapshot.progress.status == OpStatus::Pending
    });

    test_app
        .invoke_payload::<(), _>(
            "reorder_ops",
            ReorderOpsRequest {
                ids: vec![pending_two.clone(), pending_one.clone()],
            },
        )
        .expect("reorder ops");

    let order = test_app
        .queue_snapshot()
        .into_iter()
        .map(|snapshot| snapshot.progress.operation_id)
        .collect::<Vec<_>>();
    let pending_one_index = order
        .iter()
        .position(|id| id == &pending_one)
        .expect("pending one in snapshot");
    let pending_two_index = order
        .iter()
        .position(|id| id == &pending_two)
        .expect("pending two in snapshot");
    assert!(pending_two_index < pending_one_index);

    test_app
        .invoke_payload::<(), _>(
            "cancel_op",
            OpIdRequest {
                id: pending_two.clone(),
            },
        )
        .expect("cancel pending op");
    test_app.wait_for_op(&pending_two, |snapshot| {
        snapshot.progress.status == OpStatus::Cancelled
    });

    let has_unfinished_during_conflict: bool = test_app
        .invoke("has_unfinished_ops")
        .expect("unfinished ops");
    assert!(has_unfinished_during_conflict);

    test_app
        .invoke_payload::<(), _>(
            "resolve_conflict",
            ResolveConflictRequest {
                id: conflict_id.clone(),
                resolution: ConflictResolution::Replace,
                apply_to_all: false,
                rename_to: None,
            },
        )
        .expect("resolve conflict");
    test_app.wait_for_op(&conflict_id, |snapshot| {
        snapshot.progress.status == OpStatus::Completed
    });
    assert_eq!(
        fs::read(dest.join("dup.txt")).expect("resolved dest"),
        b"new-content"
    );

    test_app.wait_for_op(&pending_one, |snapshot| {
        snapshot.progress.status == OpStatus::Completed
    });

    let failed_id: String = test_app
        .invoke_payload(
            "start_op",
            StartOpRequest {
                kind: OpKind::Copy,
                destination_dir: dest.to_string_lossy().into_owned(),
                items: vec![OpItem {
                    source_path: root.join("missing.txt").to_string_lossy().into_owned(),
                    name: "missing.txt".to_string(),
                    size_bytes: 1,
                }],
            },
        )
        .expect("start failed op");
    test_app.wait_for_op(&failed_id, |snapshot| {
        snapshot.progress.status == OpStatus::Failed
    });

    test_app
        .invoke_payload::<(), _>(
            "retry_op",
            OpIdRequest {
                id: failed_id.clone(),
            },
        )
        .expect("retry failed op");
    test_app.wait_for_op(&failed_id, |snapshot| {
        snapshot.progress.status == OpStatus::Failed
    });

    let paused_source = root.join("pause.txt");
    write_file(&paused_source, &vec![b'x'; 8192]);
    let paused_id: String = test_app
        .invoke_payload(
            "start_op",
            StartOpRequest {
                kind: OpKind::Copy,
                destination_dir: dest.to_string_lossy().into_owned(),
                items: vec![op_item(&paused_source)],
            },
        )
        .expect("start pause op");
    test_app
        .invoke_payload::<(), _>(
            "pause_op",
            OpIdRequest {
                id: paused_id.clone(),
            },
        )
        .expect("pause op");
    test_app.wait_for_op(&paused_id, |snapshot| {
        matches!(
            snapshot.progress.status,
            OpStatus::Paused | OpStatus::Completed
        )
    });

    test_app
        .invoke_payload::<(), _>(
            "resume_op",
            OpIdRequest {
                id: paused_id.clone(),
            },
        )
        .expect("resume op");
    test_app.wait_for_op(&paused_id, |snapshot| {
        snapshot.progress.status == OpStatus::Completed
    });

    let unfinished_after: bool = test_app
        .invoke("has_unfinished_ops")
        .expect("has unfinished");
    assert!(matches!(unfinished_after, false | true));
    let snapshots = test_app.queue_snapshot();
    assert!(snapshots
        .iter()
        .any(|snapshot| snapshot.progress.operation_id == failed_id));
}
