#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::thread;
use std::time::Duration;

use file_explorer_lib::persist::{
    load_json_or_default, write_json_atomic, write_json_atomic_with_failure, ColumnConfig, Config,
    LayoutConfig, PersistedStore, PersistenceState, Session, SessionPane, SessionTab,
};
use tempfile::tempdir;

#[test]
fn config_and_session_round_trip_through_atomic_storage() {
    let fixture = tempdir().expect("temp dir");
    let config_path = fixture.path().join("config.json");
    let session_path = fixture.path().join("session.json");

    let config_store =
        PersistedStore::<Config>::load(config_path.clone(), Duration::from_millis(5))
            .expect("config store");
    let session_store =
        PersistedStore::<Session>::load(session_path.clone(), Duration::from_millis(5))
            .expect("session store");

    config_store.replace(Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        dismissed_everything_banner: true,
        keybindings: std::collections::HashMap::from([(
            "refresh".to_string(),
            vec!["Ctrl+R".to_string()],
        )]),
        columns: vec![
            ColumnConfig {
                key: "name".to_string(),
                visible: true,
            },
            ColumnConfig {
                key: "size".to_string(),
                visible: false,
            },
        ],
        layout: LayoutConfig {
            details_visible: false,
            tree_width_px: 320.0,
            pane_split: 0.4,
            default_pane_mode: "single".to_string(),
            restore_session: false,
            zoom: "150".to_string(),
        },
        update_check_interval: "5h".to_string(),
    });
    session_store.replace(Session {
        active_pane: "right".to_string(),
        left_path: "C:\\left".to_string(),
        right_path: "D:\\right".to_string(),
        ..Session::default()
    });

    thread::sleep(Duration::from_millis(30));

    let loaded_config: Config = load_json_or_default(&config_path).expect("load config");
    let loaded_session: Session = load_json_or_default(&session_path).expect("load session");

    assert_eq!(
        loaded_config,
        Config {
            theme: "dark".to_string(),
            show_hidden_files: true,
            dismissed_everything_banner: true,
            keybindings: std::collections::HashMap::from([(
                "refresh".to_string(),
                vec!["Ctrl+R".to_string()],
            )]),
            columns: vec![
                ColumnConfig {
                    key: "name".to_string(),
                    visible: true,
                },
                ColumnConfig {
                    key: "size".to_string(),
                    visible: false,
                },
            ],
            layout: LayoutConfig {
                details_visible: false,
                tree_width_px: 320.0,
                pane_split: 0.4,
                default_pane_mode: "single".to_string(),
                restore_session: false,
                zoom: "150".to_string(),
            },
            update_check_interval: "5h".to_string(),
        }
    );
    assert_eq!(
        loaded_session,
        Session {
            active_pane: "right".to_string(),
            left_path: "C:\\left".to_string(),
            right_path: "D:\\right".to_string(),
            ..Session::default()
        }
    );
}

#[test]
fn session_with_per_pane_tabs_round_trips_through_storage() {
    let fixture = tempdir().expect("temp dir");
    let session_path = fixture.path().join("session.json");

    let session_store =
        PersistedStore::<Session>::load(session_path.clone(), Duration::from_millis(5))
            .expect("session store");

    let session = Session {
        active_pane: "left".to_string(),
        left_path: "C:\\left".to_string(),
        right_path: "D:\\right".to_string(),
        left: Some(SessionPane {
            active_tab_index: 1,
            tabs: vec![
                SessionTab {
                    path: "C:\\left".to_string(),
                    sort_key: "name".to_string(),
                    sort_direction: "asc".to_string(),
                    filter: String::new(),
                },
                SessionTab {
                    path: "C:\\left\\nested".to_string(),
                    sort_key: "size".to_string(),
                    sort_direction: "desc".to_string(),
                    filter: "report".to_string(),
                },
            ],
        }),
        right: Some(SessionPane {
            active_tab_index: 0,
            tabs: vec![SessionTab {
                path: "D:\\right".to_string(),
                sort_key: "modified".to_string(),
                sort_direction: "desc".to_string(),
                filter: String::new(),
            }],
        }),
    };

    session_store.replace(session.clone());
    thread::sleep(Duration::from_millis(30));

    let loaded: Session = load_json_or_default(&session_path).expect("load session");
    assert_eq!(loaded, session);
}

#[test]
fn legacy_flat_session_deserializes_with_no_tab_state() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("session.json");

    fs::write(
        &path,
        br#"{"activePane":"right","leftPath":"C:\\old","rightPath":"D:\\old"}"#,
    )
    .expect("seed legacy session");

    let loaded: Session = load_json_or_default(&path).expect("load legacy session");
    assert_eq!(loaded.active_pane, "right");
    assert_eq!(loaded.left_path, "C:\\old");
    assert_eq!(loaded.right_path, "D:\\old");
    assert!(loaded.left.is_none());
    assert!(loaded.right.is_none());
}

#[test]
fn failed_write_leaves_previous_file_content_intact() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("config.json");

    fs::write(
        &path,
        serde_json::to_vec_pretty(&Config {
            theme: "light".to_string(),
            show_hidden_files: false,
            dismissed_everything_banner: false,
            ..Config::default()
        })
        .expect("serialize"),
    )
    .expect("seed config");

    let write_result = write_json_atomic_with_failure(
        &path,
        &Config {
            theme: "dark".to_string(),
            show_hidden_files: true,
            dismissed_everything_banner: false,
            ..Config::default()
        },
        true,
    );

    assert!(write_result.is_err());

    let current: Config = load_json_or_default(&path).expect("load current");
    assert_eq!(
        current,
        Config {
            theme: "light".to_string(),
            show_hidden_files: false,
            dismissed_everything_banner: false,
            ..Config::default()
        }
    );
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn legacy_config_without_layout_fields_gets_defaults() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("config.json");

    fs::write(
        &path,
        br#"{"theme":"dark","showHiddenFiles":true,"dismissedEverythingBanner":true}"#,
    )
    .expect("seed legacy config");

    let loaded: Config = load_json_or_default(&path).expect("load config");
    assert_eq!(loaded.theme, "dark");
    assert!(loaded.show_hidden_files);
    assert!(loaded.dismissed_everything_banner);
    assert!(loaded.keybindings.is_empty());
    assert_eq!(loaded.columns.len(), 6);
    assert_eq!(loaded.layout, LayoutConfig::default());
    assert_eq!(loaded.update_check_interval, "1d");
}

#[test]
fn missing_persistence_files_load_defaults_and_state_creates_directory() {
    let fixture = tempdir().expect("temp dir");
    let config_dir = fixture.path().join("nested").join("config");

    let state = PersistenceState::load(&config_dir).expect("persistence state");

    assert!(config_dir.is_dir());
    assert_eq!(state.config.current(), Config::default());
    assert_eq!(state.session.current().active_pane, "left");
}

#[test]
fn explicit_flush_writes_current_value_immediately() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("config.json");
    let store =
        PersistedStore::<Config>::load(path.clone(), Duration::from_secs(60)).expect("store");

    store.replace(Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        ..Config::default()
    });
    store.flush_now().expect("flush");

    let loaded: Config = load_json_or_default(&path).expect("load config");
    assert_eq!(loaded.theme, "dark");
    assert!(loaded.show_hidden_files);
}

#[test]
fn malformed_json_surfaces_a_serde_error() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("config.json");
    fs::write(&path, b"{not valid json").expect("seed invalid");

    let error = load_json_or_default::<Config>(&path).expect_err("invalid json");
    assert!(!error.to_string().is_empty());
}

#[test]
fn debounced_writes_keep_only_the_latest_generation() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("config.json");
    let store =
        PersistedStore::<Config>::load(path.clone(), Duration::from_millis(20)).expect("store");

    store.replace(Config {
        theme: "light".to_string(),
        show_hidden_files: false,
        ..Config::default()
    });
    store.replace(Config {
        theme: "dark".to_string(),
        show_hidden_files: true,
        ..Config::default()
    });
    thread::sleep(Duration::from_millis(80));

    let loaded: Config = load_json_or_default(&path).expect("load config");
    assert_eq!(loaded.theme, "dark");
    assert!(loaded.show_hidden_files);
}

#[test]
fn atomic_writer_creates_missing_parent_directories() {
    let fixture = tempdir().expect("temp dir");
    let path = fixture.path().join("deep").join("config.json");

    write_json_atomic(&path, &Config::default()).expect("write config");

    let loaded: Config = load_json_or_default(&path).expect("load config");
    assert_eq!(loaded, Config::default());
}
