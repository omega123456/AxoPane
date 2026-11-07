use std::sync::Arc;
use std::time::{Duration, SystemTime};

use file_explorer_lib::search::source::EverythingSource;
use file_explorer_lib::search::SearchError;
use file_explorer_lib::size::everything::{
    filetime_to_system_time, EverythingHandle, EverythingQuery, EverythingQueryResult,
    EverythingRow, EverythingSort,
};

fn query(search: &str) -> EverythingQuery {
    EverythingQuery {
        search: search.to_owned(),
        sort: EverythingSort::DateModifiedDescending,
        offset: 0,
        max: 10,
    }
}

fn row(name: &str) -> EverythingRow {
    EverythingRow {
        path: r"C:\Users\alex".to_owned(),
        file_name: name.to_owned(),
        is_folder: false,
        size: None,
        modified_filetime: None,
    }
}

#[test]
fn filetimes_convert_around_the_unix_epoch() {
    const EPOCH: u64 = 116_444_736_000_000_000;
    assert_eq!(filetime_to_system_time(0), None);
    assert_eq!(filetime_to_system_time(u64::MAX), None);
    assert_eq!(filetime_to_system_time(EPOCH), Some(SystemTime::UNIX_EPOCH));
    assert_eq!(
        filetime_to_system_time(EPOCH + 15_000_000),
        Some(SystemTime::UNIX_EPOCH + Duration::from_millis(1_500))
    );
    assert_eq!(
        filetime_to_system_time(EPOCH - 10_000_000),
        SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(1))
    );
}

#[test]
fn sort_codes_match_the_sdk() {
    assert_eq!(EverythingSort::NameAscending.code(), 1);
    assert_eq!(EverythingSort::DateModifiedDescending.code(), 14);
}

#[test]
fn the_fake_handle_records_queries_and_answers_from_its_responder() {
    let handle = EverythingHandle::test_with_responder(Arc::new(|query: &EverythingQuery| {
        (query.search != "fail").then(|| EverythingQueryResult {
            rows: vec![row(&query.search)],
            total: 3,
        })
    }));
    let answered = handle.run_query(&query("invoice")).expect("answered");
    assert_eq!(answered.rows, vec![row("invoice")]);
    assert_eq!(answered.total, 3);
    assert!(handle.run_query(&query("fail")).is_err());
    assert_eq!(
        handle.recorded_queries(),
        vec![query("invoice"), query("fail")]
    );
}

#[test]
fn fake_handles_without_a_responder_return_nothing_or_fail() {
    let empty = EverythingHandle::test_available(Default::default());
    assert_eq!(
        empty.run_query(&query("x")).expect("empty"),
        EverythingQueryResult::default()
    );
    assert!(EverythingHandle::test_available_error()
        .run_query(&query("x"))
        .is_err());
    assert!(EverythingHandle::test_not_ready()
        .run_query(&query("x"))
        .is_err());
}

#[test]
fn the_search_source_maps_handle_states_to_search_errors() {
    let ready = EverythingHandle::test_available(Default::default());
    assert_eq!(
        EverythingSource::run(&ready, &query("x")),
        Ok(EverythingQueryResult::default())
    );
    assert_eq!(
        EverythingSource::run(&EverythingHandle::test_not_ready(), &query("x")),
        Err(SearchError::NotReady)
    );
    assert!(matches!(
        EverythingSource::run(&EverythingHandle::test_available_error(), &query("x")),
        Err(SearchError::Failed(_))
    ));
}
