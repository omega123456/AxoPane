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
