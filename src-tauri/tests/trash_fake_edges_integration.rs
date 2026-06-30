use file_explorer_lib::trash::move_to_trash;

#[test]
fn fake_trash_rejects_blank_paths_without_a_file_name() {
    let error = move_to_trash(&[String::new()]).expect_err("blank path should fail");

    assert!(error.contains("invalid path"));
}
