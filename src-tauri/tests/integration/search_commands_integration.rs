use std::sync::Arc;

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    SearchPageRequest, SearchQueryRequest, SearchRejection, SearchScope,
};
use file_explorer_lib::search::service::{SearchBackend, SearchService};
use file_explorer_lib::size::everything::{
    EverythingHandle, EverythingQuery, EverythingQueryResult, EverythingRow,
};

fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
    // Safety: tauri::State is a transparent wrapper over an immutable reference
    // and these tests keep the borrowed values alive for the full call.
    unsafe { std::mem::transmute::<&'a T, tauri::State<'a, T>>(value) }
}

fn service() -> Arc<SearchService> {
    let rows: Vec<EverythingRow> = (0..55)
        .map(|index| EverythingRow {
            path: r"D:\Work".to_owned(),
            file_name: format!("report-{index:02}.docx"),
            is_folder: false,
            size: None,
            modified_filetime: None,
        })
        .collect();
    let handle = EverythingHandle::test_with_responder(Arc::new(move |_: &EverythingQuery| {
        Some(EverythingQueryResult {
            rows: rows.clone(),
            total: rows.len() as u32,
        })
    }));
    Arc::new(SearchService::new(
        SearchBackend::Everything(Arc::new(handle)),
        None,
    ))
}

#[test]
fn search_commands_query_page_and_close() {
    let search = service();
    let first = commands::search_query(
        SearchQueryRequest {
            query: "report".into(),
            include_hidden: false,
            scope: SearchScope::All,
        },
        as_state(&search),
    )
    .expect("first page");
    assert_eq!((first.total, first.items.len()), (55, 50));

    let next = commands::search_page(
        SearchPageRequest {
            query_id: first.query_id,
            offset: 50,
        },
        as_state(&search),
    )
    .expect("next page");
    assert_eq!(next.items.len(), 5);

    commands::search_close(as_state(&search));
    assert_eq!(
        commands::search_page(
            SearchPageRequest {
                query_id: first.query_id,
                offset: 0,
            },
            as_state(&search),
        ),
        Err(SearchRejection::Stale)
    );
}
