use std::path::Path;

use crate::ipc::types::{
    RequestThumbnailsRequest, RequestThumbnailsResponse, ThumbnailCandidateRequest,
    ThumbnailPriority,
};

use super::scheduler::{ThumbnailSubscriber, MAX_DESIRED_PER_SCOPE};
use super::types::{ThumbnailCandidate, ThumbnailFingerprint};
use super::ThumbnailService;

const CANDIDATE_LIMIT_EXCEEDED: &str = "thumbnail scope exceeds the candidate limit";

fn scheduled_candidate(
    candidate: ThumbnailCandidateRequest,
) -> (ThumbnailCandidate, ThumbnailPriority, u32) {
    let fingerprint = ThumbnailFingerprint::from_metadata(
        Path::new(&candidate.path),
        candidate.modified_unix_seconds,
        candidate.size_bytes,
    );
    (
        ThumbnailCandidate::new(fingerprint, candidate.is_directory),
        candidate.priority,
        candidate.order,
    )
}

pub fn request_thumbnails(
    service: &ThumbnailService,
    payload: RequestThumbnailsRequest,
) -> Result<RequestThumbnailsResponse, String> {
    if payload.candidates.len() > MAX_DESIRED_PER_SCOPE {
        return Err(CANDIDATE_LIMIT_EXCEEDED.into());
    }
    let subscriber = ThumbnailSubscriber {
        pane_id: payload.pane_id,
        tab_id: payload.tab_id,
        path: payload.path,
        generation: payload.generation,
    };
    let candidates = payload
        .candidates
        .into_iter()
        .map(scheduled_candidate)
        .collect();
    let accepted_count = service.replace_scope(subscriber, payload.revision, candidates);
    Ok(RequestThumbnailsResponse {
        revision: payload.revision,
        accepted_count,
    })
}
