use file_explorer_lib::native_drag::{drag_paths, start_native_drag, StartNativeDragRequest};
use tempfile::tempdir;

fn request(paths: &[&std::path::Path]) -> StartNativeDragRequest {
    StartNativeDragRequest {
        paths: paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    }
}

#[test]
fn absolute_paths_are_projected_in_order() {
    let fixture = tempdir().expect("temp dir");
    let first = fixture.path().join("alpha.txt");
    let second = fixture.path().join("beta.txt");

    let paths = drag_paths(&request(&[&first, &second])).expect("drag paths");

    assert_eq!(paths, vec![first, second]);
}

#[test]
fn an_empty_selection_is_rejected() {
    let error = drag_paths(&StartNativeDragRequest { paths: Vec::new() })
        .expect_err("empty selection rejected");

    assert!(error.contains("at least one path"), "unexpected: {error}");
}

#[test]
fn relative_paths_are_rejected() {
    let error = drag_paths(&StartNativeDragRequest {
        paths: vec!["notes/todo.txt".to_string()],
    })
    .expect_err("relative path rejected");

    assert!(error.contains("absolute paths"), "unexpected: {error}");
}

/// A drag session is process-global and modal, so the `test-utils` build must
/// validate the request and stop there rather than asking the OS for one.
#[test]
fn the_command_validates_without_starting_an_os_drag_session() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("alpha.txt");

    assert_eq!(start_native_drag(request(&[&path])), Ok(()));
    assert!(start_native_drag(StartNativeDragRequest { paths: Vec::new() }).is_err());
}
