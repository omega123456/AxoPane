mod volumes {
    pub use file_explorer_lib::volumes::*;
}

mod fs_src {
    include!("../src/fs/mod.rs");

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::tempdir;

        #[test]
        fn private_helpers_handle_types_and_natural_sort() {
            assert_eq!(infer_type_label("archive.zip", false), "ZIP file");
            assert_eq!(infer_type_label("folder", true), "Folder");
            assert_eq!(
                natural_name_compare("file2.txt", "file10.txt"),
                Ordering::Less
            );
            assert_eq!(compare_optional_u64(Some(4), None), Ordering::Greater);
            assert_eq!(
                compare_optional_string(Some("b"), Some("a")),
                Ordering::Greater
            );
        }

        #[test]
        fn helper_formats_timestamps_and_attributes() {
            let fixture = tempdir().expect("temp dir");
            let file_path = fixture.path().join(".hidden.txt");
            std::fs::write(&file_path, b"hello").expect("file");
            let mut permissions = std::fs::metadata(&file_path)
                .expect("metadata")
                .permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&file_path, permissions).expect("readonly");

            let metadata = std::fs::metadata(&file_path).expect("metadata");
            let attributes = collect_attributes(&file_path, &metadata);
            assert!(attributes.iter().any(|attribute| attribute == "readonly"));
            #[cfg(not(windows))]
            assert!(attributes.iter().any(|attribute| attribute == "hidden"));

            let formatted =
                system_time_to_rfc3339(Some(std::time::SystemTime::UNIX_EPOCH)).expect("timestamp");
            assert!(formatted.starts_with("1970-01-01T00:00:00"));
        }

        #[test]
        fn read_item_count_reports_child_count() {
            let fixture = tempdir().expect("temp dir");
            std::fs::create_dir(fixture.path().join("nested")).expect("nested");
            std::fs::write(fixture.path().join("alpha.txt"), b"a").expect("alpha");
            assert_eq!(read_item_count(fixture.path()), Some(2));
        }
    }
}
