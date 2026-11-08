use std::sync::Arc;

use file_explorer_lib::ipc::types::{
    RequestThumbnailsRequest, ThumbnailCandidateRequest, ThumbnailPriority,
};
use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::thumbnails::request::request_thumbnails;
use file_explorer_lib::thumbnails::scheduler::MAX_DESIRED_PER_SCOPE;
use file_explorer_lib::thumbnails::ThumbnailService;

const REVISION: u64 = 3;

fn candidate_request(path: &str, is_directory: bool) -> ThumbnailCandidateRequest {
    ThumbnailCandidateRequest {
        path: path.into(),
        modified_unix_seconds: 0,
        size_bytes: 8,
        is_directory,
        priority: ThumbnailPriority::Visible,
        order: 0,
    }
}

fn scope_request(candidates: Vec<ThumbnailCandidateRequest>) -> RequestThumbnailsRequest {
    RequestThumbnailsRequest {
        pane_id: "left".into(),
        tab_id: "tab".into(),
        path: "/folder".into(),
        generation: 1,
        revision: REVISION,
        candidates,
    }
}

fn service() -> ThumbnailService {
    ThumbnailService::new(Arc::new(ResourceCoordinator::new()))
}

#[test]
fn request_echoes_the_revision_and_skips_directories() {
    let request = scope_request(vec![
        candidate_request("/folder/image.png", false),
        candidate_request("/folder/child", true),
    ]);
    let response = request_thumbnails(&service(), request).expect("accepted request");
    assert_eq!(response.revision, REVISION);
    assert_eq!(response.accepted_count, 2);
}

#[test]
fn request_rejects_scopes_over_the_candidate_limit() {
    let oversized = (0..=MAX_DESIRED_PER_SCOPE)
        .map(|index| candidate_request(&format!("/folder/{index}.png"), false))
        .collect();
    let error = request_thumbnails(&service(), scope_request(oversized)).expect_err("rejected");
    assert_eq!(error, "thumbnail scope exceeds the candidate limit");
}
