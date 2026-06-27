mod fs {
    pub use file_explorer_lib::fs::*;
}

mod persist_src {
    include!("../src/persist/mod.rs");

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn private_defaults_match_expected_values() {
            assert_eq!(default_update_check_interval(), "1d");
            assert_eq!(default_zoom(), "100");
            assert_eq!(default_tree_width_px(), 204.0);
            assert_eq!(default_pane_split(), 0.5);
            assert_eq!(default_sort_key(), "name");
            assert_eq!(default_sort_direction(), "asc");
            assert_eq!(default_columns().len(), 6);
        }
    }
}
