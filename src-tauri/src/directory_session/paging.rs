//! Idempotent page-index range materialization and baseline/revision
//! validation. Pure functions over [`SessionView`]/[`SessionBaseline`] data —
//! no locking or session lifecycle here, that lives in `directory_session::mod`.

use super::model::{
    GetSessionRangeRequest, SessionBaseline, SessionRangePage, SessionRangeResponse,
    SessionRejection, SESSION_PAGE_SIZE,
};
use super::view::SessionView;
use super::PaneTabKey;

pub struct ExpectedSession<'session> {
    pub key: &'session PaneTabKey,
    pub baseline: SessionBaseline,
    pub view: &'session SessionView,
}

/// Validates that `request` targets exactly `expected` (pane, tab, and every
/// revision field), then materializes the requested page from `view`.
/// Re-requesting the same `page_index` against the same `expected` baseline
/// always returns the same content, because `view` is immutable for the
/// lifetime of one baseline.
pub fn materialize_range(
    request: &GetSessionRangeRequest,
    expected_session: ExpectedSession<'_>,
) -> Result<SessionRangeResponse, SessionRejection> {
    let ExpectedSession {
        key,
        baseline: expected,
        view,
    } = expected_session;
    let targets_expected_tab = request.pane_id == key.pane_id && request.tab_id == key.tab_id;
    if !targets_expected_tab {
        // Wrong pane/tab entirely: treat as "no active session" from the
        // caller's point of view rather than leaking cross-pane state.
        return Err(SessionRejection::NoActiveSession);
    }

    validate_baseline(request.baseline, expected)?;

    let page_entries = view
        .page(request.page_index, SESSION_PAGE_SIZE)
        .ok_or(SessionRejection::PageOutOfRange)?;

    Ok(SessionRangeResponse {
        baseline: expected,
        total_rows: view.total_rows(),
        page: SessionRangePage {
            page_index: request.page_index,
            entries: page_entries.to_vec(),
        },
    })
}

/// Rejects a request/response baseline that does not exactly match the
/// currently-active one, field by field, so the caller learns precisely which
/// revision went stale (session replaced vs. navigation superseded vs. watch
/// re-armed vs. view changed).
pub fn validate_baseline(
    supplied: SessionBaseline,
    expected: SessionBaseline,
) -> Result<(), SessionRejection> {
    if supplied.session_id != expected.session_id {
        return Err(SessionRejection::StaleSession);
    }
    if supplied.navigation_revision != expected.navigation_revision {
        return Err(SessionRejection::StaleNavigation);
    }
    if supplied.watch_revision != expected.watch_revision {
        return Err(SessionRejection::StaleWatch);
    }
    if supplied.view_revision != expected.view_revision {
        return Err(SessionRejection::StaleView);
    }
    Ok(())
}
