//! Collects Everything candidates for a query: strict passes, a fuzzy pass
//! only when they found little, and a two-step child listing for
//! `folder/` queries.

use std::collections::HashSet;

use super::everything_plan::{self as plan, EverythingPass};
use super::path_style::PathStyle;
use super::query::ParsedQuery;
use super::results::{rank_candidates, BROWSE_PARENT_CAP, CANDIDATE_CAP};
use super::source::EverythingSource;
use super::{CandidateBatch, Control, SearchError, SearchOptions, SearchRun, SearchScope};
use crate::size::everything::{EverythingQuery, EverythingSort};

/// The fuzzy pass runs only while fewer candidates than this were found.
pub const FUZZY_PASS_THRESHOLD: usize = 200;

pub type EverythingSearchRun<'run> = SearchRun<'run, dyn EverythingSource>;

pub fn candidates(
    run: &EverythingSearchRun<'_>,
    query: &ParsedQuery,
) -> Result<CandidateBatch, SearchError> {
    if !query.browse {
        return collect(run.source, &plan::passes(query, run.options), run.control);
    }
    let parents = match &query.anchor {
        Some(anchor) => vec![anchor.path(PathStyle::Windows)],
        None => browse_parents(run, &query.browse_parents())?,
    };
    let listings: Vec<EverythingPass> = plan::browse_children_queries(&parents, run.options)
        .into_iter()
        .map(|search| EverythingPass {
            search,
            fuzzy: false,
        })
        .collect();
    collect(run.source, &listings, run.control)
}

/// The best-ranked folders matching the query's folder segments.
fn browse_parents(
    run: &EverythingSearchRun<'_>,
    parent_query: &ParsedQuery,
) -> Result<Vec<String>, SearchError> {
    let folders = SearchOptions {
        scope: SearchScope::Folders,
        ..*run.options
    };
    let batch = collect(
        run.source,
        &plan::passes(parent_query, &folders),
        run.control,
    )?;
    Ok(
        rank_candidates(batch.candidates, parent_query, PathStyle::Windows)
            .into_iter()
            .take(BROWSE_PARENT_CAP)
            .map(|hit| PathStyle::Windows.strip_verbatim(&hit.path))
            .collect(),
    )
}

fn collect(
    source: &dyn EverythingSource,
    passes: &[EverythingPass],
    control: &Control<'_>,
) -> Result<CandidateBatch, SearchError> {
    let mut seen = HashSet::new();
    let mut batch = CandidateBatch::default();
    for pass in passes {
        let found = batch.candidates.len();
        let fuzzy_pass_unneeded = pass.fuzzy && found >= FUZZY_PASS_THRESHOLD;
        if fuzzy_pass_unneeded {
            continue;
        }
        let must_stop = !control.proceed()? || found >= CANDIDATE_CAP;
        if must_stop {
            batch.truncated = true;
            break;
        }
        let result = source.run(&EverythingQuery {
            search: pass.search.clone(),
            sort: EverythingSort::DateModifiedDescending,
            offset: 0,
            max: (CANDIDATE_CAP - batch.candidates.len()) as u32,
        })?;
        batch.truncated |= result.total as usize > result.rows.len();
        let unseen = result
            .rows
            .into_iter()
            .filter_map(plan::row_to_candidate)
            .filter(|candidate| seen.insert(candidate.path.clone()));
        batch.candidates.extend(unseen);
    }
    Ok(batch)
}
