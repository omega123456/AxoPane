use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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
}

pub fn default_update_check_interval() -> String {
    "1d".to_string()
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
    #[serde(default = "default_pane_split")]
    pub pane_split: f64,
    #[serde(default = "default_column_widths")]
    pub column_widths: HashMap<String, f64>,
    pub default_pane_mode: String,
    pub restore_session: bool,
    #[serde(default = "default_zoom")]
    pub zoom: String,
}

pub fn default_zoom() -> String {
    "100".to_string()
}

pub fn default_tree_width_px() -> f64 {
    204.0
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
            pane_split: default_pane_split(),
            column_widths: default_column_widths(),
            default_pane_mode: "dual".to_string(),
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
    /// Optional richer per-pane tab state. Absent in legacy sessions, in which
    /// case the flat `left_path`/`right_path` fields are used to rebuild a
    /// single tab per pane on the frontend.
    #[serde(default)]
    pub left: Option<SessionPane>,
    #[serde(default)]
    pub right: Option<SessionPane>,
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
    write_generation: Arc<Mutex<u64>>,
    debounce: Duration,
    _marker: PhantomData<T>,
}

impl<T> PersistedStore<T>
where
    T: Clone + Default + Serialize + DeserializeOwned + Send + 'static,
{
    pub fn load(path: PathBuf, debounce: Duration) -> Result<Self, PersistError> {
        let value = load_json_or_default(&path)?;

        Ok(Self {
            path,
            value: Arc::new(Mutex::new(value)),
            write_generation: Arc::new(Mutex::new(0)),
            debounce,
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
        let generation = {
            let mut current_generation = self
                .write_generation
                .lock()
                .expect("persist generation mutex poisoned");
            *current_generation += 1;
            *current_generation
        };

        let path = self.path.clone();
        let value = Arc::clone(&self.value);
        let generation_state = Arc::clone(&self.write_generation);
        let debounce = self.debounce;

        thread::spawn(move || {
            thread::sleep(debounce);

            let should_write = {
                let latest_generation = generation_state
                    .lock()
                    .expect("persist generation mutex poisoned");
                *latest_generation == generation
            };

            if !should_write {
                return;
            }

            let snapshot = value.lock().expect("persist store mutex poisoned").clone();
            let _ = write_json_atomic(&path, &snapshot);
        });
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
            config: PersistedStore::load(
                config_dir.join("config.json"),
                Duration::from_millis(200),
            )?,
            session: PersistedStore::load(
                config_dir.join("session.json"),
                Duration::from_millis(200),
            )?,
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
    replace_file_atomic(&temp_path, path)?;

    Ok(())
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
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    let source_wide = wide(source.as_os_str());
    let target_wide = wide(target.as_os_str());

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
