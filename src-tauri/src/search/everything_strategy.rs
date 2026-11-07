//! Collects Everything candidates for a query: strict passes, a fuzzy pass
//! only when they found little, and a two-step child listing for
//! `folder/` queries.

use std::collections::HashSet;

use super::everything_plan::{self as plan, EverythingPass};
use super::path_style::PathStyle;
use super::query::ParsedQuery;
use super::results::{rank_candidates, BROWSE_PARENT_CAP, CANDIDATE_CAP};
use super::source::EverythingSource;
use super::{CandidateBatch, Control, SearchError, SearchOptions, SearchScope};
use crate::size::everything::{EverythingQuery, EverythingSort};

/// The fuzzy pass runs only while fewer candidates than this were found.
pub const FUZZY_PASS_THRESHOLD: usize = 200;

pub fn candidates(
    source: &dyn EverythingSource,
    query: &ParsedQuery,
    options: &SearchOptions,
    control: &Control<'_>,
) -> Result<CandidateBatch, SearchError> {
    if !query.browse {
        return collect(source, &plan::passes(query, options), control);
    }
    let parents = match &query.anchor {
        Some(anchor) => vec![anchor.path(PathStyle::Windows)],
        None => browse_parents(source, &query.browse_parents(), options, control)?,
    };
    let listings: Vec<EverythingPass> = plan::browse_children_queries(&parents, options)
        .into_iter()
        .map(|search| EverythingPass {
            search,
            fuzzy: false,
        })
        .collect();
    collect(source, &listings, control)
}

/// The best-ranked folders matching the query's folder segments.
fn browse_parents(
    source: &dyn EverythingSource,
    parent_query: &ParsedQuery,
    options: &SearchOptions,
    control: &Control<'_>,
) -> Result<Vec<String>, SearchError> {
    let folders = SearchOptions {
        scope: SearchScope::Folders,
        ..*options
    };
    let batch = collect(source, &plan::passes(parent_query, &folders), control)?;
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
        if pass.fuzzy && batch.candidates.len() >= FUZZY_PASS_THRESHOLD {
            continue;
        }
        if !control.proceed()? || batch.candidates.len() >= CANDIDATE_CAP {
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
        for row in result.rows {
            if let Some(candidate) = plan::row_to_candidate(row) {
                if seen.insert(candidate.path.clone()) {
                    batch.candidates.push(candidate);
                }
            }
        }
    }
    Ok(batch)
}
