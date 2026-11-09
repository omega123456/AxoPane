use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const FLUSH_DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub theme: String,
    pub show_hidden_files: bool,
    #[serde(default)]
    pub dismissed_everything_banner: bool,
    #[serde(default)]
    pub keybindings: HashMap<String, Vec<String>>,
    #[serde(default = "default_columns")]
    pub columns: Vec<ColumnConfig>,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default = "default_update_check_interval")]
    pub update_check_interval: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_date_format")]
    pub date_format: String,
    #[serde(default)]
    pub show_time: bool,
    #[serde(default)]
    pub show_seconds: bool,
    #[serde(default)]
    pub relative_dates: bool,
    #[serde(default = "default_true")]
    pub auto_folder_size: bool,
    #[serde(default)]
    pub auto_expand_active_queue_toasts: bool,
    #[serde(default)]
    pub favourites: Vec<String>,
    #[serde(default)]
    pub favourite_names: HashMap<String, String>,
    #[serde(default)]
    pub favourites_collapsed: bool,
}

pub fn default_true() -> bool {
    true
}

pub fn default_update_check_interval() -> String {
    "1d".to_string()
}

pub fn default_log_level() -> String {
    "info".to_string()
}

pub fn default_date_format() -> String {
    "ymd".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnConfig {
    pub key: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutConfig {
    pub details_visible: bool,
    #[serde(default = "default_tree_width_px")]
    pub tree_width_px: f64,
    #[serde(default = "default_favourites_height_px")]
    pub favourites_height_px: f64,
    #[serde(default = "default_pane_split")]
    pub pane_split: f64,
    #[serde(default = "default_column_widths")]
    pub column_widths: HashMap<String, f64>,
    pub default_pane_mode: String,
    #[serde(default = "default_view_mode")]
    pub default_view_mode: String,
    pub restore_session: bool,
    #[serde(default = "default_zoom")]
    pub zoom: String,
}

pub fn default_zoom() -> String {
    "100".to_string()
}

pub fn default_view_mode() -> String {
    "details".to_string()
}

pub fn default_tree_width_px() -> f64 {
    212.0
}

pub fn default_favourites_height_px() -> f64 {
    112.0
}

pub fn default_pane_split() -> f64 {
    0.5
}

pub fn default_column_widths() -> HashMap<String, f64> {
    HashMap::from([
        ("name".to_string(), 320.0),
        ("size".to_string(), 96.0),
        ("items".to_string(), 72.0),
        ("type".to_string(), 136.0),
        ("modified".to_string(), 128.0),
        ("created".to_string(), 128.0),
    ])
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            details_visible: true,
            tree_width_px: default_tree_width_px(),
            favourites_height_px: default_favourites_height_px(),
            pane_split: default_pane_split(),
            column_widths: default_column_widths(),
            default_pane_mode: "dual".to_string(),
            default_view_mode: default_view_mode(),
            restore_session: true,
            zoom: default_zoom(),
        }
    }
}

pub fn default_columns() -> Vec<ColumnConfig> {
    vec![
        ColumnConfig {
            key: "name".to_string(),
            visible: true,
        },
        ColumnConfig {
            key: "size".to_string(),
            visible: true,
        },
        ColumnConfig {
            key: "items".to_string(),
            visible: true,
        },
        ColumnConfig {
            key: "type".to_string(),
            visible: true,
        },
        ColumnConfig {
            key: "modified".to_string(),
            visible: true,
        },
        ColumnConfig {
            key: "created".to_string(),
            visible: false,
        },
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            show_hidden_files: false,
            dismissed_everything_banner: false,
            keybindings: HashMap::new(),
            columns: default_columns(),
            layout: LayoutConfig::default(),
            update_check_interval: default_update_check_interval(),
            log_level: default_log_level(),
            date_format: default_date_format(),
            show_time: false,
            show_seconds: false,
            relative_dates: false,
            auto_folder_size: true,
            auto_expand_active_queue_toasts: false,
            favourites: Vec::new(),
            favourite_names: HashMap::new(),
            favourites_collapsed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTab {
    pub path: String,
    #[serde(default = "default_sort_key")]
    pub sort_key: String,
    #[serde(default = "default_sort_direction")]
    pub sort_direction: String,
    #[serde(default)]
    pub filter: String,
    #[serde(default)]
    pub view_mode: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

pub fn default_sort_key() -> String {
    "name".to_string()
}

pub fn default_sort_direction() -> String {
    "asc".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionPane {
    #[serde(default)]
    pub active_tab_index: usize,
    #[serde(default)]
    pub tabs: Vec<SessionTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub active_pane: String,
    pub left_path: String,
    pub right_path: String,
    #[serde(default)]
    pub left: Option<SessionPane>,
    #[serde(default)]
    pub right: Option<SessionPane>,
    #[serde(default)]
    pub recent_folders: Vec<String>,
}

impl Default for Session {
    fn default() -> Self {
        let start = crate::fs::default_start_dir()
            .to_string_lossy()
            .into_owned();
        Self {
            active_pane: "left".to_string(),
            left_path: start.clone(),
            right_path: start,
            left: None,
            right: None,
            recent_folders: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum PersistError {
    Io(std::io::Error),
    Serde(serde_json::Error),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Serde(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PersistError {}

impl From<std::io::Error> for PersistError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for PersistError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}

#[derive(Clone)]
pub struct PersistedStore<T> {
    path: PathBuf,
    value: Arc<Mutex<T>>,
    flush_wake: Sender<()>,
    _marker: PhantomData<T>,
}

impl<T> PersistedStore<T>
where
    T: Clone + Default + Serialize + DeserializeOwned + Send + 'static,
{
    pub fn load(path: PathBuf, debounce: Duration) -> Result<Self, PersistError> {
        let value: T = load_json_or_default(&path)?;
        let value = Arc::new(Mutex::new(value));
        let (flush_wake, flush_receiver) = bounded(1);
        let worker = FlushWorker {
            receiver: flush_receiver,
            debounce,
            path: path.clone(),
            value: Arc::clone(&value),
        };
        thread::Builder::new()
            .name("persistence-flush".to_string())
            .spawn(move || worker.run())
            .expect("failed to spawn persistence worker");

        Ok(Self {
            path,
            value,
            flush_wake,
            _marker: PhantomData,
        })
    }

    pub fn current(&self) -> T {
        self.value
            .lock()
            .expect("persist store mutex poisoned")
            .clone()
    }

    pub fn replace(&self, next_value: T) {
        {
            let mut current = self.value.lock().expect("persist store mutex poisoned");
            *current = next_value;
        }

        self.schedule_flush();
    }

    pub fn flush_now(&self) -> Result<(), PersistError> {
        let current = self.current();
        write_json_atomic(&self.path, &current)
    }

    fn schedule_flush(&self) {
        let _ = self.flush_wake.try_send(());
    }
}

#[derive(Clone)]
pub struct PersistenceState {
    pub config: PersistedStore<Config>,
    pub session: PersistedStore<Session>,
}

impl PersistenceState {
    pub fn load(config_dir: &Path) -> Result<Self, PersistError> {
        fs::create_dir_all(config_dir)?;

        Ok(Self {
            config: PersistedStore::load(config_dir.join("config.json"), FLUSH_DEBOUNCE)?,
            session: PersistedStore::load(config_dir.join("session.json"), FLUSH_DEBOUNCE)?,
        })
    }
}

pub fn load_json_or_default<T>(path: &Path) -> Result<T, PersistError>
where
    T: Default + DeserializeOwned,
{
    if !path.exists() {
        return Ok(T::default());
    }

    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_json_atomic<T>(path: &Path, value: &T) -> Result<(), PersistError>
where
    T: Serialize,
{
    write_json_atomic_with_failure(path, value, false)
}

pub fn write_json_atomic_with_failure<T>(
    path: &Path,
    value: &T,
    fail_before_replace: bool,
) -> Result<(), PersistError>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let serialized = serde_json::to_vec_pretty(value)?;
    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));

    fs::write(&temp_path, serialized)?;

    if fail_before_replace {
        return Err(PersistError::Io(std::io::Error::other(
            "simulated failure before atomic replace",
        )));
    }

    replace_file_atomic(&temp_path, path)
}

#[cfg(windows)]
fn replace_file_atomic(source: &Path, target: &Path) -> Result<(), PersistError> {
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = crate::win32::wide_nul(source);
    let target_wide = crate::win32::wide_nul(target);

    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if moved == 0 {
        return Err(PersistError::Io(std::io::Error::last_os_error()));
    }

    Ok(())
}

#[cfg(not(windows))]
fn replace_file_atomic(source: &Path, target: &Path) -> Result<(), PersistError> {
    fs::rename(source, target)?;
    Ok(())
}

struct FlushWorker<T> {
    receiver: Receiver<()>,
    debounce: Duration,
    path: PathBuf,
    value: Arc<Mutex<T>>,
}

impl<T: Serialize + Clone> FlushWorker<T> {
    fn run(self) {
        while self.flush_is_due() {
            let snapshot = self
                .value
                .lock()
                .expect("persist store mutex poisoned")
                .clone();
            let _ = write_json_atomic(&self.path, &snapshot);
        }
    }

    fn flush_is_due(&self) -> bool {
        self.receiver.recv().is_ok() && self.wait_for_quiet_period()
    }

    fn wait_for_quiet_period(&self) -> bool {
        loop {
            match self.receiver.recv_timeout(self.debounce) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => return true,
                Err(RecvTimeoutError::Disconnected) => return false,
            }
        }
    }
}
