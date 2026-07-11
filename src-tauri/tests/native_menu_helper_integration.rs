#[path = "common/mod.rs"]
mod common;

use file_explorer_lib::ipc::types::{LoadNativeMenuRequest, NativeMenuTargetKind};
use file_explorer_lib::native_menu::helper_entry::{dispatch_for_tests, run_framed_stdio};
use file_explorer_lib::native_menu::helper_protocol::{
    read_frame, write_frame, HelperOperation, HelperRequest, MAX_FRAME_BYTES,
};
use file_explorer_lib::native_menu::helper_supervisor::{
    HelperFailure, HelperRole, HelperSupervisor, DISCOVERY_DEADLINE,
};
use file_explorer_lib::native_menu::menu_cache::MenuCache;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

fn request(id: &str, extension: &str) -> LoadNativeMenuRequest {
    let path = format!("C:\\fixture\\file.{extension}");
    LoadNativeMenuRequest {
        request_id: id.to_string(),
        target_kind: NativeMenuTargetKind::File,
        target_path: Some(path.clone()),
        folder_path: Some("C:\\fixture".to_string()),
        selected_paths: vec![path],
    }
}

#[test]
fn framed_protocol_round_trips_request_ids_and_rejects_oversized_input() {
    assert_eq!(common::bootstrap_message(), "phase-1-common");
    let request = HelperRequest {
        request_id: 42,
        operation: HelperOperation::Discover(request("one", "txt")),
    };
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &request).expect("bounded frame writes");
    let decoded: HelperRequest = read_frame(&mut Cursor::new(bytes)).expect("frame reads");
    assert_eq!(decoded, request);
    let oversized = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec();
    assert!(read_frame::<_, HelperRequest>(&mut Cursor::new(oversized)).is_err());
}

#[test]
fn cache_caps_distinct_menu_types_at_128_entries() {
    let cache = MenuCache::default();
    for index in 0..129 {
        cache.insert(&request(&format!("r{index}"), &format!("x{index}")), &[]);
    }
    assert_eq!(cache.len(), 128);
    assert!(cache.get(&request("new", "x128")).is_some());
    assert!(cache.get(&request("old", "x0")).is_none());
}

#[test]
fn test_utils_supervisor_never_starts_a_real_shell_helper() {
    let outcome = HelperSupervisor::default().call(
        HelperRole::Warm,
        HelperOperation::Discover(request("safe", "txt")),
        DISCOVERY_DEADLINE,
    );
    assert_eq!(outcome, Err(HelperFailure::Unsupported));
}

#[test]
fn test_utils_helper_dispatch_never_reaches_native_shell_code() {
    let result = dispatch_for_tests(HelperOperation::Discover(request("safe-dispatch", "txt")));
    assert!(matches!(
        result,
        file_explorer_lib::native_menu::helper_protocol::HelperResult::Status(status)
            if !status.handled && status.message.as_deref() == Some("unsupported")
    ));
}

#[test]
fn framed_stdio_dispatches_a_safe_test_utils_response_then_accepts_eof() {
    let request = HelperRequest {
        request_id: 73,
        operation: HelperOperation::Discover(request("stdio", "txt")),
    };
    let mut input = Vec::new();
    write_frame(&mut input, &request).expect("request frame");
    let mut output = Vec::new();
    run_framed_stdio(Cursor::new(input), &mut output).expect("stdio helper completes at EOF");
    let response =
        read_frame::<_, file_explorer_lib::native_menu::helper_protocol::HelperResponse>(
            &mut Cursor::new(output),
        )
        .expect("response frame");
    assert_eq!(response.request_id, 73);
    assert!(matches!(
        response.result,
        file_explorer_lib::native_menu::helper_protocol::HelperResult::Status(_)
    ));
}

#[test]
fn equivalent_warm_cache_loads_are_single_flight_and_path_independent() {
    let cache = Arc::new(MenuCache::default());
    let first_request = request("first", "txt");
    let second_request = LoadNativeMenuRequest {
        request_id: "second".to_string(),
        target_kind: NativeMenuTargetKind::File,
        target_path: Some("C:\\other\\same.txt".to_string()),
        folder_path: Some("C:\\other".to_string()),
        selected_paths: vec!["C:\\other\\same.txt".to_string()],
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);

    let leader_cache = Arc::clone(&cache);
    let leader_calls = Arc::clone(&calls);
    let leader = thread::spawn(move || {
        leader_cache.get_or_load(&first_request, || {
            leader_calls.fetch_add(1, Ordering::SeqCst);
            started_tx.send(()).expect("leader announces load");
            release_rx.recv().expect("leader released");
            Vec::new()
        })
    });
    started_rx.recv().expect("leader is loading");

    let waiter_cache = Arc::clone(&cache);
    let waiter = thread::spawn(move || waiter_cache.get_or_load(&second_request, Vec::new));
    release_tx.send(()).expect("release leader");

    assert!(leader.join().expect("leader result").1);
    assert!(!waiter.join().expect("waiter result").1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
