use file_explorer_lib::archive;
use file_explorer_lib::ipc::types::{CompressArchiveRequest, ExtractArchiveRequest};
use std::fs;
use tempfile::tempdir;

#[cfg(all(
    any(target_os = "windows", target_os = "macos"),
    not(feature = "test-utils")
))]
#[test]
fn compresses_and_extracts_zip_archives_in_a_temp_directory() {
    let fixture = tempdir().expect("temp dir");
    let source_root = fixture.path().join("source");
    let destination_root = fixture.path().join("dest");
    fs::create_dir_all(source_root.join("nested")).expect("source tree");
    fs::create_dir_all(&destination_root).expect("destination dir");

    fs::write(
        source_root.join("nested").join("note.txt"),
        "hello from zip",
    )
    .expect("nested file");
    fs::write(source_root.join("root.txt"), "top level").expect("root file");

    let compress = archive::compress_archive(CompressArchiveRequest {
        paths: vec![source_root.to_string_lossy().into_owned()],
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });
    assert!(compress.handled);

    let archive_path = compress.message.expect("archive path");
    assert!(archive_path.ends_with(".zip"));
    assert!(fs::metadata(&archive_path).is_ok());

    let extract = archive::extract_archive(ExtractArchiveRequest {
        paths: vec![archive_path.clone()],
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });
    assert!(extract.handled);

    let extracted_root = extract.message.expect("extracted root");
    assert_eq!(
        fs::read_to_string(
            std::path::Path::new(&extracted_root)
                .join("nested")
                .join("note.txt")
        )
        .expect("nested extract"),
        "hello from zip"
    );
    assert_eq!(
        fs::read_to_string(std::path::Path::new(&extracted_root).join("root.txt"))
            .expect("root extract"),
        "top level"
    );
}

#[cfg(feature = "test-utils")]
#[test]
fn archive_commands_report_unsupported_under_test_utils() {
    let fixture = tempdir().expect("temp dir");
    let destination_root = fixture.path().join("dest");
    fs::create_dir_all(&destination_root).expect("destination dir");

    let compress = archive::compress_archive(CompressArchiveRequest {
        paths: vec![fixture.path().join("source").to_string_lossy().into_owned()],
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });
    assert!(!compress.handled);
    assert_eq!(compress.message.as_deref(), Some("unsupported"));

    let extract = archive::extract_archive(ExtractArchiveRequest {
        paths: vec![fixture
            .path()
            .join("archive.zip")
            .to_string_lossy()
            .into_owned()],
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });
    assert!(!extract.handled);
    assert_eq!(extract.message.as_deref(), Some("unsupported"));
}

#[test]
fn reports_safe_status_for_invalid_archive_requests() {
    let fixture = tempdir().expect("temp dir");
    let destination_root = fixture.path().join("dest");
    fs::create_dir_all(&destination_root).expect("destination dir");

    let compress = archive::compress_archive(CompressArchiveRequest {
        paths: Vec::new(),
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });
    assert!(!compress.handled);
    #[cfg(feature = "test-utils")]
    assert_eq!(compress.message.as_deref(), Some("unsupported"));
    #[cfg(not(feature = "test-utils"))]
    assert_eq!(compress.message.as_deref(), Some("invalid-request"));

    let extract = archive::extract_archive(ExtractArchiveRequest {
        paths: vec![fixture
            .path()
            .join("missing.zip")
            .to_string_lossy()
            .into_owned()],
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });
    assert!(!extract.handled);
    #[cfg(feature = "test-utils")]
    assert_eq!(extract.message.as_deref(), Some("unsupported"));
    #[cfg(not(feature = "test-utils"))]
    assert_eq!(extract.message.as_deref(), Some("source-not-found"));
}

#[cfg(all(
    any(target_os = "windows", target_os = "macos"),
    not(feature = "test-utils")
))]
#[test]
fn rejects_non_zip_extract_requests() {
    let fixture = tempdir().expect("temp dir");
    let destination_root = fixture.path().join("dest");
    let archive_path = fixture.path().join("archive.rar");
    fs::create_dir_all(&destination_root).expect("destination dir");
    fs::write(&archive_path, "not a zip").expect("placeholder archive");

    let extract = archive::extract_archive(ExtractArchiveRequest {
        paths: vec![archive_path.to_string_lossy().into_owned()],
        destination_dir: destination_root.to_string_lossy().into_owned(),
    });

    assert!(!extract.handled);
    assert_eq!(
        extract.message.as_deref(),
        Some("unsupported-archive-format")
    );
}
