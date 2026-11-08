use crate::search::service::SearchService;
use std::sync::Arc;

use crate::ipc::types::{SearchPage, SearchPageRequest, SearchQueryRequest, SearchRejection};
use tauri::State;

/// Runs a search everywhere query; a newer query supersedes it (`stale`).
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn search_query(
    payload: SearchQueryRequest,
    search: State<'_, Arc<SearchService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<SearchPage, SearchRejection> {
    let service = Arc::clone(search.inner());
    let ticket = service.begin();
    let cancellation = ticket.cancellation.clone();
    executor
        .latency_cancellable_with_deadline(
            "search".to_string(),
            cancellation,
            crate::search::service::SEARCH_DEADLINE,
            move |cancel| Ok(service.run(ticket, &payload, &|| cancel.is_cancelled())),
        )
        .await
        .map_err(SearchRejection::from_executor)?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn search_query(
    payload: SearchQueryRequest,
    search: State<'_, Arc<SearchService>>,
) -> Result<SearchPage, SearchRejection> {
    let ticket = search.begin();
    search.run(ticket, &payload, &|| false)
}

/// Serves another page of the latest query's results.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn search_page(
    payload: SearchPageRequest,
    search: State<'_, Arc<SearchService>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<SearchPage, SearchRejection> {
    let service = Arc::clone(search.inner());
    executor
        .latency(
            "search-page".to_string(),
            move || Ok(service.page(&payload)),
        )
        .await
        .map_err(SearchRejection::from_executor)?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn search_page(
    payload: SearchPageRequest,
    search: State<'_, Arc<SearchService>>,
) -> Result<SearchPage, SearchRejection> {
    search.page(&payload)
}

#[tauri::command]
pub fn search_close(search: State<'_, Arc<SearchService>>) {
    search.close();
}
