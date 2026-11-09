use crate::search::service::{RunTicket, SearchService};
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
        .latency_cancellable(
            crate::ipc::executor::LatencyRequest::cancellable("search".to_string(), cancellation)
                .with_deadline(crate::search::service::SEARCH_DEADLINE),
            move |cancel| {
                let run_ticket = RunTicket {
                    ticket,
                    cancelled: &|| cancel.is_cancelled(),
                };
                Ok(service.run(run_ticket, &payload))
            },
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
    let run_ticket = RunTicket {
        ticket: search.begin(),
        cancelled: &|| false,
    };
    search.run(run_ticket, &payload)
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
