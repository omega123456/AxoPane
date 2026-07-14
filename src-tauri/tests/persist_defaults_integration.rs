#[path = "common/mod.rs"]
mod common;

use file_explorer_lib::persist::{
    default_column_widths, default_columns, default_date_format, default_pane_split,
    default_sort_direction, default_sort_key, default_tree_width_px, default_update_check_interval,
    default_view_mode, default_zoom, Config, LayoutConfig,
};

#[test]
fn persistence_defaults_match_the_frontend_contract() {
    assert_eq!(default_update_check_interval(), "1d");
    assert_eq!(default_date_format(), "ymd");
    assert_eq!(default_zoom(), "100");
    assert_eq!(default_view_mode(), "details");
    assert_eq!(default_tree_width_px(), 204.0);
    assert_eq!(default_pane_split(), 0.5);
    assert_eq!(default_column_widths().get("name"), Some(&320.0));
    assert_eq!(default_column_widths().get("type"), Some(&136.0));
    assert_eq!(default_sort_key(), "name");
    assert_eq!(default_sort_direction(), "asc");

    let columns = default_columns();
    assert_eq!(columns.len(), 6);
    assert_eq!(columns[0].key, "name");
    assert!(columns[0].visible);
    assert_eq!(columns[5].key, "created");
    assert!(!columns[5].visible);

    let config = Config::default();
    assert_eq!(config.theme, "system");
    assert!(!config.show_hidden_files);
    assert_eq!(config.date_format, "ymd");
    assert!(!config.show_time);
    assert!(!config.show_seconds);
    assert!(!config.relative_dates);
    assert!(config.auto_folder_size);
    assert!(!config.auto_expand_active_queue_toasts);
    assert_eq!(config.columns, columns);
    assert_eq!(config.layout, LayoutConfig::default());
    assert_eq!(config.layout.default_view_mode, "details");
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}
