use std::fs;
use std::path::{Path, PathBuf};

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    AddIndexExclusionRequest, AddIndexExclusionResponse, IndexExclusionOutcomeKind,
    RemoveIndexExclusionRequest,
};
use file_explorer_lib::persist::PersistenceState;
use file_explorer_lib::search::service::{SearchBackend, SearchService};
use file_explorer_lib::search::SearchScope;
use file_explorer_lib::size::index::exclusions::{
    is_within, plan_reconcile, same_path, ExclusionRules, ExclusionSet, ExclusionVerdict,
    PathVerdict, ReconcilePlan,
};
use file_explorer_lib::size::index::persist;
use file_explorer_lib::size::index::platform::{RawEvent, DATA_VOLUME_PREFIX};
use file_explorer_lib::size::index::tree::FolderTotal;
use file_explorer_lib::size::{IndexState, RequestMode, SizeSource, SizeStateKind};
use tempfile::tempdir;

use crate::ipc_state_manager_integration::as_state;
use crate::size_index_service_integration::{
    fresh_total, live_search, ready, terminal, wait_until, Harness,
};

const MAC_ALWAYS_SKIPPED: [&str; 3] = ["/System/Volumes", "/Volumes", "/dev"];
const COMPOSED_CAFE: &str = "caf\u{e9}";
const DECOMPOSED_CAFE: &str = "cafe\u{301}";
const CURRENT_EVENT_ID: u64 = 100;

fn verdicts(outcomes: &[PathVerdict]) -> Vec<ExclusionVerdict> {
    outcomes
        .iter()
        .map(|outcome| outcome.verdict.clone())
        .collect()
}

fn harness_rules(index: &Harness) -> ExclusionRules {
    ExclusionRules {
        roots: index.roots.clone(),
        always_skipped: index.excluded.clone(),
        alias_prefix: Some(PathBuf::from(DATA_VOLUME_PREFIX)),
        home: index.home.clone(),
    }
}

fn tree_total(index: &Harness, relative: &str) -> Option<FolderTotal> {
    let path = index.path(relative);
    index
        .service()
        .folder_size_index()
        .expect("index attached")
        .read_index(|view| view.tree.total(&path))
        .expect("readable")
}

fn search_paths(index: &Harness, query: &str) -> Vec<String> {
    let search = SearchService::new(SearchBackend::from_sizes(index.service()), None);
    live_search(&search, query, SearchScope::All)
        .items
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

fn add(index: &Harness, relatives: &[&str]) -> Vec<PathVerdict> {
    let candidates: Vec<PathBuf> = relatives
        .iter()
        .map(|relative| index.path(relative))
        .collect();
    index
        .service()
        .add_index_exclusions(&candidates)
        .expect("index attached")
        .verdicts
}

fn finish_history_replay(index: &Harness) {
    let history_done = RawEvent {
        path: index.path(""),
        id: CURRENT_EVENT_ID,
        flags: persist::ChangeFlags::default(),
        history_done: true,
    };
    assert!(index.platform.emit(vec![history_done]), "stream running");
}

fn validation_harness() -> Harness {
    let mut index = Harness::new();
    fs::create_dir_all(index.path("a/b/c")).unwrap();
    fs::create_dir_all(index.path("never-indexed")).unwrap();
    index.excluded = vec![index.path("never-indexed")];
    index.home = Some(index.path("home"));
    index
}

#[test]
fn each_validation_rule_produces_its_outcome() {
    let index = validation_harness();
    let rules = harness_rules(&index);
    let mut set = ExclusionSet::default();
    let add_one =
        |set: &mut ExclusionSet, relative: &str| set.add(&index.path(relative), &rules).verdict;

    assert_eq!(add_one(&mut set, "a/b"), ExclusionVerdict::Added);
    assert_eq!(
        add_one(&mut set, "a/b"),
        ExclusionVerdict::Duplicate(index.path("a/b"))
    );
    assert_eq!(
        add_one(&mut set, "a/b/c"),
        ExclusionVerdict::InsideExisting(index.path("a/b"))
    );
    assert_eq!(
        add_one(&mut set, "a"),
        ExclusionVerdict::MergedChildren(vec![index.path("a/b")])
    );
    assert_eq!(set.paths(), &[index.path("a")]);
    assert_eq!(
        add_one(&mut set, "never-indexed"),
        ExclusionVerdict::AlwaysSkipped
    );
    assert_eq!(add_one(&mut set, ""), ExclusionVerdict::RootRejected);
    assert_eq!(add_one(&mut set, "home"), ExclusionVerdict::HomeWarning);
    assert_eq!(add_one(&mut set, "missing"), ExclusionVerdict::NotFound);
    assert_eq!(add_one(&mut set, "top.bin"), ExclusionVerdict::NotAFolder);
    assert_eq!(set.paths(), &[index.path("a"), index.path("home")]);
}

#[test]
fn rejected_typed_paths_report_the_text_that_was_typed() {
    let index = validation_harness();
    let mut set = ExclusionSet::default();
    let typed = index.path("missing/child");
    let outcome = set.add(&typed, &harness_rules(&index));
    assert_eq!(outcome.path, typed);
    assert!(!outcome.verdict.adds_path());
    assert!(set.paths().is_empty());
}

#[test]
fn macos_system_paths_and_the_data_volume_alias_are_classified() {
    let temp = tempdir().expect("temp dir");
    let canonical = temp.path().canonicalize().expect("canonical");
    let rules = ExclusionRules {
        roots: vec![PathBuf::from("/")],
        always_skipped: MAC_ALWAYS_SKIPPED.iter().map(PathBuf::from).collect(),
        alias_prefix: Some(PathBuf::from(DATA_VOLUME_PREFIX)),
        home: None,
    };
    let mut set = ExclusionSet::default();
    for always_skipped in ["/Volumes", "/dev", DATA_VOLUME_PREFIX] {
        assert_eq!(
            set.add(Path::new(always_skipped), &rules).verdict,
            ExclusionVerdict::AlwaysSkipped,
            "{always_skipped}"
        );
    }
    assert_eq!(
        set.add(Path::new("/"), &rules).verdict,
        ExclusionVerdict::RootRejected
    );
    let aliased = Path::new(DATA_VOLUME_PREFIX).join(canonical.strip_prefix("/").unwrap());
    let outcome = set.add(&aliased, &rules);
    assert_eq!(outcome.verdict, ExclusionVerdict::Added);
    assert_eq!(outcome.path, canonical, "stored in the walker's form");
}

#[test]
fn comparisons_fall_back_to_case_and_unicode_folding() {
    let composed = Path::new("/Users/alex").join(COMPOSED_CAFE);
    let decomposed = Path::new("/users/ALEX").join(DECOMPOSED_CAFE);
    assert!(same_path(&composed, &decomposed));
    assert!(is_within(&decomposed.join("menu"), &composed));
    assert!(!is_within(Path::new("/Users/alex/cafeteria"), &composed));

    let mut set = ExclusionSet::new(vec![composed.clone()]);
    assert_eq!(set.matching(&decomposed), Some(&composed));
    assert_eq!(set.covering(&decomposed.join("menu")), Some(&composed));
    assert_eq!(set.remove(&decomposed), Some(composed));
    assert_eq!(set.remove(Path::new("/elsewhere")), None);
}

#[test]
fn reconciliation_prunes_new_exclusions_and_walks_removed_ones() {
    let stored = vec![
        PathBuf::from("/old"),
        PathBuf::from("/kept"),
        PathBuf::from("/x/y"),
    ];
    let configured = vec![
        PathBuf::from("/kept"),
        PathBuf::from("/new"),
        PathBuf::from("/x"),
    ];
    assert_eq!(
        plan_reconcile(&stored, &configured),
        ReconcilePlan {
            prune: vec![PathBuf::from("/new"), PathBuf::from("/x")],
            walk: vec![PathBuf::from("/old")],
        }
    );
    assert_eq!(
        plan_reconcile(&configured, &configured),
        ReconcilePlan::default()
    );
}

#[test]
fn multi_path_adds_report_one_outcome_per_path_in_order() {
    let mut index = validation_harness();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let outcomes = add(&index, &["a/b", "a/b/c", "top.bin", "s"]);
    assert_eq!(
        verdicts(&outcomes),
        vec![
            ExclusionVerdict::Added,
            ExclusionVerdict::InsideExisting(index.path("a/b")),
            ExclusionVerdict::NotAFolder,
            ExclusionVerdict::Added,
        ]
    );
    let exclusions = index
        .service()
        .folder_size_index()
        .expect("index attached")
        .exclusions();
    assert_eq!(exclusions, vec![index.path("a/b"), index.path("s")]);
}

#[test]
fn adding_an_exclusion_prunes_it_to_a_findable_skipped_leaf() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert!(search_paths(&index, "b.bin").contains(&index.key("a/b/b.bin")));

    add(&index, &["a"]);

    let excluded = tree_total(&index, "a").expect("kept as a leaf");
    assert!(excluded.skipped && excluded.bytes == 0);
    assert!(tree_total(&index, "a/b").is_none());
    assert!(search_paths(&index, "b.bin").is_empty());
    assert!(search_paths(&index, "a.bin").is_empty());
    assert!(search_paths(&index, "idx/a").contains(&index.key("a")));

    let root = terminal(&index.request(&[""], RequestMode::Auto), &index.key(""));
    assert!(root.is_lower_bound);
    assert_eq!(root.excluded_roots, Some(vec!["a".to_string()]));
    assert_eq!(root.skipped_roots, Some(vec!["Documents".to_string()]));
    assert_eq!(
        root.size_bytes,
        Some(fresh_total(&index.root) - fresh_total(&index.path("a")) - 1000)
    );

    let auto = terminal(
        &index.request(&["a/b"], RequestMode::Auto),
        &index.key("a/b"),
    );
    assert_eq!(auto.state, SizeStateKind::Unknown);
    let manual = terminal(&index.request(&["a"], RequestMode::Manual), &index.key("a"));
    assert_eq!(manual.source, SizeSource::Manual);
    assert_eq!(manual.size_bytes, Some(110));
}

#[test]
fn removing_an_exclusion_rewalks_it_to_exact_totals() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    add(&index, &["a"]);
    index.opened.lock().unwrap().clear();

    let remaining = index
        .service()
        .remove_index_exclusion(&index.path("a"))
        .expect("index attached");
    assert!(remaining.is_empty());

    wait_until("a re-walked", || index.opened("a/b"));
    index.wait_state(IndexState::UpToDate);
    assert_eq!(ready(&index, "a"), 110);
    assert!(search_paths(&index, "b.bin").contains(&index.key("a/b/b.bin")));
    let root = terminal(&index.request(&[""], RequestMode::Auto), &index.key(""));
    assert_eq!(root.excluded_roots, None);
}

#[test]
fn records_from_a_walk_in_flight_cannot_recreate_an_excluded_subtree() {
    let mut index = Harness::new();
    index.blocker.block(&index.path("a/b"));
    index.start();
    wait_until("walk reaches a/b", || index.opened("a/b"));

    add(&index, &["a"]);
    index.blocker.release(&index.path("a/b"));
    index.wait_state(IndexState::UpToDate);

    assert!(tree_total(&index, "a").expect("leaf").skipped);
    assert!(tree_total(&index, "a/b").is_none());
    assert!(search_paths(&index, "b.bin").is_empty());
}

#[test]
fn an_exclusion_cancels_a_boost_running_inside_it_and_answers_unknown() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.blocker.block(&index.path("a/b"));
    let boost = index.request(&["a"], RequestMode::Refresh);
    wait_until("boost running", || index.opened("a/b"));

    add(&index, &["a"]);
    index.blocker.release(&index.path("a/b"));

    assert_eq!(
        terminal(&boost, &index.key("a")).state,
        SizeStateKind::Unknown
    );
}

#[test]
fn events_and_rereads_inside_an_exclusion_are_dropped() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    add(&index, &["a"]);
    fs::create_dir(index.path("a/fresh")).unwrap();
    index.opened.lock().unwrap().clear();

    index.event("a", persist::ChangeFlags::default());
    index.event("", persist::ChangeFlags::default());
    index.service().flush_index();

    assert!(!index.opened("a"));
    assert!(tree_total(&index, "a/fresh").is_none());
    assert!(tree_total(&index, "a").expect("leaf").skipped);
}

#[test]
fn exclusions_survive_a_restart_and_are_reconciled_without_a_full_rescan() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.service().flush_index();
    index.stop();

    index.user_exclusions = vec![index.path("a")];
    index.opened.lock().unwrap().clear();
    index.start();
    finish_history_replay(&index);
    index.wait_state(IndexState::UpToDate);
    assert!(tree_total(&index, "a").expect("leaf").skipped);
    assert!(tree_total(&index, "a/b").is_none());
    assert!(!index.opened(""), "no full rescan");
    index.service().flush_index();
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.exclusions, vec![index.path("a")]);
    index.stop();

    index.user_exclusions = Vec::new();
    index.opened.lock().unwrap().clear();
    index.start();
    finish_history_replay(&index);
    wait_until("a re-walked", || index.opened("a/b"));
    index.wait_state(IndexState::UpToDate);
    assert!(!index.opened(""), "no full rescan");
    assert_eq!(ready(&index, "a"), 110);
}

fn add_through_command(
    index: &Harness,
    persistence: &PersistenceState,
    paths: Vec<String>,
) -> AddIndexExclusionResponse {
    commands::add_index_exclusion(
        AddIndexExclusionRequest { paths },
        as_state(index.service()),
        as_state(persistence),
    )
    .expect("supported")
}

#[test]
fn exclusion_commands_validate_apply_and_persist_through_the_index() {
    let mut index = validation_harness();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let config_dir = tempdir().expect("config dir");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");

    let added = add_through_command(
        &index,
        &persistence,
        vec![index.key("a/b"), index.key("a"), index.key("a/b/c")],
    );
    let kinds: Vec<IndexExclusionOutcomeKind> = added
        .outcomes
        .iter()
        .map(|outcome| outcome.outcome)
        .collect();
    assert_eq!(
        kinds,
        vec![
            IndexExclusionOutcomeKind::Added,
            IndexExclusionOutcomeKind::MergedChildren,
            IndexExclusionOutcomeKind::InsideExisting,
        ]
    );
    assert_eq!(added.outcomes[1].merged_count, Some(1));
    assert_eq!(added.outcomes[2].detail, Some(index.key("a")));
    assert_eq!(added.exclusions, vec![index.key("a")]);
    assert_eq!(
        persistence.config.current().index_exclusions,
        vec![index.key("a")]
    );

    let duplicate = add_through_command(&index, &persistence, vec![index.key("a")]);
    assert_eq!(
        duplicate.outcomes[0].outcome,
        IndexExclusionOutcomeKind::Duplicate
    );
    assert_eq!(duplicate.outcomes[0].detail, Some(index.key("a")));

    let removed = commands::remove_index_exclusion(
        RemoveIndexExclusionRequest {
            path: index.key("a"),
        },
        as_state(index.service()),
        as_state(&persistence),
    )
    .expect("supported");
    assert!(removed.exclusions.is_empty());
    assert!(persistence.config.current().index_exclusions.is_empty());

    let picked = commands::pick_index_exclusions(as_state(index.service())).expect("supported");
    assert!(picked.paths.is_empty(), "the test picker selects nothing");
    assert_eq!(
        commands::retry_folder_size_index(as_state(index.service())),
        Ok(())
    );
}
