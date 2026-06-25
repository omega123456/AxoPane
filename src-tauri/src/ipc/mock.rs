use super::types::{AppConfig, InitialShellResponse, PaneShell, SessionState, TreeRoot};

pub fn initial_shell() -> InitialShellResponse {
    InitialShellResponse {
        panes: vec![
            PaneShell {
                id: "left".to_string(),
                title: "Left pane".to_string(),
                path: "C:\\Users\\Omega".to_string(),
                placeholder_heading: "Filesystem services are stubbed".to_string(),
                placeholder_body:
                    "Phase 2 will replace this placeholder shell with real directory data."
                        .to_string(),
            },
            PaneShell {
                id: "right".to_string(),
                title: "Right pane".to_string(),
                path: "D:\\projects".to_string(),
                placeholder_heading: "IPC contracts are ready".to_string(),
                placeholder_body:
                    "Commands, event channels, stores, and test mocks are established in Phase 1."
                        .to_string(),
            },
        ],
        tree_roots: vec![
            TreeRoot {
                id: "this-pc".to_string(),
                label: "This PC".to_string(),
            },
            TreeRoot {
                id: "projects".to_string(),
                label: "Projects".to_string(),
            },
        ],
    }
}

pub fn config() -> AppConfig {
    AppConfig {
        theme: "system".to_string(),
        show_hidden_files: false,
        dismissed_everything_banner: false,
        ..AppConfig::default()
    }
}

pub fn session() -> SessionState {
    SessionState {
        active_pane: "left".to_string(),
        left_path: "C:\\Users\\Omega".to_string(),
        right_path: "D:\\projects".to_string(),
        left: None,
        right: None,
    }
}
