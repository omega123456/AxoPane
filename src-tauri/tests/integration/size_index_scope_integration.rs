//! The index scope: off, folders only, or folders and files. macOS only.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::SaveConfigRequest;
use file_explorer_lib::persist::{Config, PersistenceState};
use file_explorer_lib::search::source::FolderTreeSource;
use file_explorer_lib::search::SearchError;
use file_explorer_lib::size::index::names::file::spill_path;
use file_explorer_lib::size::index::names_path_beside;
use file_explorer_lib::size::index::persist::{self, ChangeFlags};
use file_explorer_lib::size::index::platform::RawEvent;
use file_explorer_lib::size::{
    FolderSizeIndexStatus, IndexBackend, IndexScope, IndexState, IndexUnsupported, SizeBackend,
};
use tempfile::tempdir;

use crate::ipc_state_manager_integration::as_state;
use crate::size_index_service_integration::{fresh_total, ready, wait_until, Harness};

const FIRST_EVENT_ID: u64 = 100;
const REPLAYED_EVENT_ID: u64 = 300;
const BYTES_IN_A: u64 = 110;
const BYTES_IN_A_WITHOUT_B: u64 = 10;

fn folders_only() -> Harness {
    let mut index = Harness::new();
    index.collect_file_names = false;
    index
}

fn names_file(index: &Harness) -> PathBuf {
    names_path_beside(&index.snapshot)
}

fn saved_pair_token(index: &Harness) -> u64 {
    let snapshot = persist::load(&index.snapshot).expect("saved snapshot");
    snapshot.meta.pair_token
}

fn status(index: &Harness) -> FolderSizeIndexStatus {
    index.service().folder_size_index_status()
}

fn off_status() -> FolderSizeIndexStatus {
    FolderSizeIndexStatus::new(IndexBackend::MacIndex, false, IndexState::Off)
}

fn forget_opened_folders(index: &Harness) {
    index.opened.lock().unwrap().clear();
}

fn finish_history_replay(index: &Harness) {
    let history_done = RawEvent {
        path: index.path(""),
        id: REPLAYED_EVENT_ID,
        flags: ChangeFlags::default(),
        history_done: true,
    };
    assert!(index.platform.emit(vec![history_done]), "stream running");
}

/// Switches the scope and waits for the restarted index to open its stream.
fn restart_with_scope(index: &Harness, scope: IndexScope) {
    let starts_before = index.platform.stream_starts().len();
    index.service().set_index_scope(scope);
    wait_until("stream restart", || {
        index.platform.stream_starts().len() > starts_before
    });
}

fn indexed_file_names(index: &Harness, relative: &str) -> Option<Vec<String>> {
    let path = index.path(relative);
    let running = index.service().folder_size_index().expect("index running");
    running
        .read_index(|view| {
            let slot = view.tree.lookup(&path)?;
            let names = view.names.names_of(slot).ok()??;
            let listed = names
                .names()
                .map(|name| String::from_utf8_lossy(name).into_owned())
                .collect();
            Some(listed)
        })
        .expect("readable")
}

#[test]
fn scope_follows_the_two_toggles_and_files_imply_folders() {
    assert_eq!(IndexScope::from_toggles(false, false), IndexScope::Off);
    assert_eq!(IndexScope::from_toggles(true, false), IndexScope::Folders);
    assert_eq!(
        IndexScope::from_toggles(true, true),
        IndexScope::FoldersAndFiles
    );
    assert_eq!(
        IndexScope::from_toggles(false, true),
        IndexScope::FoldersAndFiles
    );
    assert!(IndexScope::FoldersAndFiles.collects_file_names());
    assert!(!IndexScope::Folders.collects_file_names());
    assert!(!IndexScope::Off.collects_file_names());
}

#[test]
fn a_folder_only_index_sizes_folders_and_saves_no_file_names() {
    let mut index = folders_only();
    index.start();
    index.wait_state(IndexState::UpToDate);

    assert_eq!(ready(&index, "a"), fresh_total(&index.path("a")));
    assert!(status(&index).is_available);
    assert!(!status(&index).file_names_available);
    assert_eq!(indexed_file_names(&index, "a"), None);
    let storage = index.service().folder_size_index_storage();
    assert_eq!(storage.expect("index running").file_count, None);

    index.service().flush_index();
    assert_eq!(saved_pair_token(&index), persist::NO_PAIR);
    assert!(!names_file(&index).exists());
    assert!(!spill_path(&names_file(&index)).exists());
}

#[test]
fn a_folder_only_index_resumes_from_its_snapshot_without_rescanning() {
    let mut index = folders_only();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.stop();
    forget_opened_folders(&index);

    index.start();
    finish_history_replay(&index);
    index.wait_state(IndexState::UpToDate);

    assert_eq!(
        index.platform.stream_starts().last(),
        Some(&Some(FIRST_EVENT_ID))
    );
    assert!(!index.opened(""), "no full rescan");
    assert_eq!(ready(&index, "a"), BYTES_IN_A);
}

#[test]
fn a_folder_only_index_follows_removed_and_excluded_folders() {
    let mut index = folders_only();
    index.start();
    index.wait_state(IndexState::UpToDate);

    fs::remove_dir_all(index.path("a/b")).unwrap();
    index.event("a/b", ChangeFlags::default());
    index.event("a", ChangeFlags::default());
    index.service().flush_index();
    assert_eq!(ready(&index, "a"), BYTES_IN_A_WITHOUT_B);

    let excluded = index.service().add_index_exclusions(&[index.path("s")]);
    assert_eq!(
        excluded.expect("index running").exclusions,
        [index.path("s")]
    );
    index.service().window_focus_changed(false);
    index.service().flush_index();

    let saved = persist::load(&index.snapshot).expect("saved snapshot");
    assert_eq!(saved.meta.exclusions, [index.path("s")]);
    assert_eq!(saved.meta.pair_token, persist::NO_PAIR);
    assert!(!names_file(&index).exists());
}

#[test]
fn turning_the_file_index_on_rescans_and_saves_the_file_names() {
    let mut index = folders_only();
    index.start();
    index.wait_state(IndexState::UpToDate);
    forget_opened_folders(&index);

    restart_with_scope(&index, IndexScope::FoldersAndFiles);
    wait_until("file names indexed", || {
        status(&index).file_names_available && index.state() == IndexState::UpToDate
    });

    assert!(index.opened(""), "a full rescan collects the names");
    assert_eq!(
        indexed_file_names(&index, "a"),
        Some(vec!["a.bin".to_string()])
    );
    assert!(names_file(&index).exists());
    assert_ne!(saved_pair_token(&index), persist::NO_PAIR);
}

#[test]
fn turning_the_file_index_off_removes_saved_names_and_keeps_the_sizes() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert!(names_file(&index).exists());
    forget_opened_folders(&index);

    restart_with_scope(&index, IndexScope::Folders);
    finish_history_replay(&index);
    index.wait_state(IndexState::UpToDate);

    assert!(!names_file(&index).exists());
    assert!(!status(&index).file_names_available);
    assert!(!index.opened(""), "sizes resume from the snapshot");
    assert_eq!(ready(&index, "a"), BYTES_IN_A);
    index.service().flush_index();
    assert_eq!(saved_pair_token(&index), persist::NO_PAIR);
}

#[test]
fn turning_the_index_off_stops_it_and_reports_off() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);

    index.service().set_index_scope(IndexScope::Off);

    assert_eq!(status(&index), off_status());
    assert_eq!(index.statuses.lock().unwrap().last(), Some(&off_status()));
    assert!(index.service().folder_size_index().is_none());
    assert_eq!(
        index.service().folder_size_index_storage(),
        Err(IndexUnsupported)
    );
    assert_eq!(
        index
            .service()
            .choose_backend_for_path(&index.path("a"), &[]),
        SizeBackend::Manual
    );
    let host = index.service().index_host().expect("index host");
    assert_eq!(host.scope(), IndexScope::Off);
    assert_eq!(
        FolderTreeSource::read(host.as_ref(), &mut |_| {}),
        Err(SearchError::Unavailable)
    );
    assert!(index.snapshot.exists(), "the stopped index stays on disk");
}

#[test]
fn turning_the_index_back_on_resumes_from_the_saved_pair() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.service().set_index_scope(IndexScope::Off);
    forget_opened_folders(&index);

    restart_with_scope(&index, IndexScope::FoldersAndFiles);
    finish_history_replay(&index);
    index.wait_state(IndexState::UpToDate);

    assert!(!index.opened(""), "no full rescan");
    assert!(status(&index).file_names_available);
    let host = index.service().index_host().expect("index host");
    let mut folders_seen = 0;
    FolderTreeSource::read(host.as_ref(), &mut |view| {
        folders_seen = view.tree.node_count();
    })
    .expect("search reads the running index");
    assert!(folders_seen > 0);
}

#[test]
fn setting_the_current_scope_keeps_the_running_index() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let running_before = index.service().folder_size_index().expect("index running");
    let starts_before = index.platform.stream_starts().len();

    index.service().set_index_scope(IndexScope::FoldersAndFiles);

    let running_after = index.service().folder_size_index().expect("index running");
    assert!(Arc::ptr_eq(&running_before, &running_after));
    assert_eq!(index.platform.stream_starts().len(), starts_before);
}

#[test]
fn saving_the_config_applies_its_index_toggles() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");
    let both_off = Config::default();

    let saved = commands::save_config(
        SaveConfigRequest { config: both_off },
        as_state(&persistence),
        as_state(index.service()),
    );

    assert!(!saved.index_folders);
    assert!(!saved.index_files);
    assert_eq!(index.state(), IndexState::Off);
}
