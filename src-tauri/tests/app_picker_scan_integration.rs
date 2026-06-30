//! `app_picker::scan` is pure and takes its roots as a parameter, so these
//! tests build a synthetic directory tree under a `tempfile::TempDir` instead
//! of touching the real `/Applications`, satisfying the project's rule that
//! tests never depend on real machine-global filesystem state.

use file_explorer_lib::app_picker::scan::scan_app_roots;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_app(root: &std::path::Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(&path).expect("create synthetic app bundle dir");
    path
}

#[test]
fn finds_top_level_app_bundles_and_utilities_subfolder() {
    let root = tempdir().expect("tempdir");
    let foo = make_app(root.path(), "Foo.app");
    let bar = make_app(root.path(), "Bar.app");
    let baz = make_app(root.path(), "Utilities/Baz.app");
    fs::create_dir_all(root.path().join("Not An App")).expect("create non-app dir");
    fs::write(root.path().join("readme.txt"), b"noise").expect("write noise file");

    let mut found = scan_app_roots(&[root.path().to_path_buf()]);
    found.sort();

    let mut expected = vec![foo, bar, baz];
    expected.sort();

    assert_eq!(found, expected);
}

#[test]
fn ignores_nested_app_bundles_beyond_utilities() {
    let root = tempdir().expect("tempdir");
    make_app(root.path(), "Outer.app/Contents/Resources/Inner.app");

    let found = scan_app_roots(&[root.path().to_path_buf()]);

    assert_eq!(found, vec![root.path().join("Outer.app")]);
}

#[test]
fn missing_root_yields_empty_without_erroring() {
    let root = tempdir().expect("tempdir");
    let missing = root.path().join("does-not-exist");

    let found = scan_app_roots(&[missing]);

    assert!(found.is_empty());
}

#[test]
fn dedupes_and_sorts_across_multiple_roots() {
    let root_a = tempdir().expect("tempdir a");
    let root_b = tempdir().expect("tempdir b");
    make_app(root_a.path(), "Zeta.app");
    make_app(root_a.path(), "Alpha.app");
    make_app(root_b.path(), "Mid.app");

    let found = scan_app_roots(&[root_a.path().to_path_buf(), root_b.path().to_path_buf()]);

    let mut sorted_clone = found.clone();
    sorted_clone.sort();
    assert_eq!(found, sorted_clone, "results must already be sorted");
    assert_eq!(found.len(), 3);
}
