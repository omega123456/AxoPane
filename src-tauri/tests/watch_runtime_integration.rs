use file_explorer_lib::watch::{add_watch, create_runtime, remove_watch};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn watch_runtime_tracks_reference_counts_for_paths() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");

    add_watch(&mut runtime, root).expect("first add");
    add_watch(&mut runtime, root).expect("second add");
    assert_eq!(runtime.watch_counts.get(root), Some(&2));

    remove_watch(&mut runtime, root).expect("first remove");
    assert_eq!(runtime.watch_counts.get(root), Some(&1));

    remove_watch(&mut runtime, root).expect("second remove");
    assert!(!runtime.watch_counts.contains_key(root));

    remove_watch(&mut runtime, root).expect("missing remove is noop");
}
