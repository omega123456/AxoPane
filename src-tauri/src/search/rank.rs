//! Scores a candidate against a parsed query and orders the hits: lower
//! summed tiers first, then fewer folder levels between the matched folder
//! and the item, folders before files, shorter paths, newer items and name.

use std::cmp::Ordering;
use std::time::SystemTime;

use super::matcher::{nfc, FoldedName, NameKind};
use super::path_style::PathStyle;
use super::query::ParsedQuery;
use super::Candidate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scored {
    /// The backend's path, used for every action.
    pub path: String,
    /// NFC display name and parent path; the ranges index into these.
    pub name: String,
    pub parent: String,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
    /// Sum of the matched segments' tiers.
    pub penalty: u32,
    /// Folder levels skipped between matched segments and the item.
    pub gap: u32,
    /// Number of path components.
    pub depth: u32,
    pub name_ranges: Vec<(u32, u32)>,
    pub parent_ranges: Vec<(u32, u32)>,
}

pub fn score(candidate: &Candidate, query: &ParsedQuery, style: PathStyle) -> Option<Scored> {
    let clean = style.strip_verbatim(&candidate.path);
    let parts = style.split(&clean);
    let (raw_name, raw_ancestors) = parts.components.split_last()?;
    let kind = if candidate.is_dir {
        NameKind::Folder
    } else {
        NameKind::File
    };
    let name = nfc(raw_name).into_owned();
    let name_match = match &query.name_seg {
        Some(segment) => Some(segment.matches(&FoldedName::new(&name), kind)?),
        None => None,
    };
    let ancestors: Vec<String> = raw_ancestors
        .iter()
        .map(|ancestor| nfc(ancestor).into_owned())
        .collect();
    let mut penalty = name_match
        .as_ref()
        .map_or(0, |name_match| u32::from(name_match.tier));
    let mut gap = 0;
    let mut hits: Vec<(usize, Vec<(u32, u32)>)> = Vec::new();
    if let Some(anchor) = &query.anchor {
        let names: Vec<&str> = ancestors.iter().map(String::as_str).collect();
        if !anchor.contains(&parts.root, &names) {
            return None;
        }
        gap = (ancestors.len() - anchor.components.len()) as u32;
        if query.browse && gap != 0 {
            return None;
        }
    } else {
        let mut upper = ancestors.len();
        for (segment_index, segment) in query.folder_segs.iter().enumerate().rev() {
            // Listing children: the innermost folder must be the parent.
            let lowest = if query.browse && segment_index + 1 == query.folder_segs.len() {
                upper.checked_sub(1)?
            } else {
                0
            };
            let mut best: Option<(usize, super::matcher::Match)> = None;
            for ancestor_index in (lowest..upper).rev() {
                let Some(found) = segment.matches(
                    &FoldedName::new(&ancestors[ancestor_index]),
                    NameKind::Folder,
                ) else {
                    continue;
                };
                if best
                    .as_ref()
                    .is_none_or(|(_, best_match)| found.tier < best_match.tier)
                {
                    let exact = found.tier == super::matcher::EXACT;
                    best = Some((ancestor_index, found));
                    if exact {
                        break;
                    }
                }
            }
            let (ancestor_index, found) = best?;
            gap += (upper - 1 - ancestor_index) as u32;
            penalty += u32::from(found.tier);
            hits.push((ancestor_index, found.ranges));
            upper = ancestor_index;
        }
    }
    let (parent, offsets) = display_parent(style, &parts.root, &ancestors);
    let mut parent_ranges: Vec<(u32, u32)> = hits
        .into_iter()
        .flat_map(|(ancestor_index, ranges)| {
            let base = offsets[ancestor_index];
            ranges
                .into_iter()
                .map(move |(start, end)| (start + base, end + base))
        })
        .collect();
    parent_ranges.sort_unstable();
    Some(Scored {
        path: candidate.path.clone(),
        name,
        parent,
        is_dir: candidate.is_dir,
        modified: candidate.modified,
        penalty,
        gap,
        depth: parts.components.len() as u32,
        name_ranges: name_match
            .map(|name_match| name_match.ranges)
            .unwrap_or_default(),
        parent_ranges,
    })
}

/// The parent path for display, plus the UTF-16 offset of each ancestor
/// name within it.
fn display_parent(style: PathStyle, root: &str, ancestors: &[String]) -> (String, Vec<u32>) {
    let mut text = nfc(root).into_owned();
    let mut offsets = Vec::with_capacity(ancestors.len());
    for name in ancestors {
        if !text.is_empty() && !text.ends_with(|character| style.is_separator(character)) {
            text.push(style.separator());
        }
        offsets.push(text.encode_utf16().count() as u32);
        text.push_str(name);
    }
    (text, offsets)
}

/// A total order over hits; a missing modified time sorts after known ones.
pub fn compare(left: &Scored, right: &Scored) -> Ordering {
    left.penalty
        .cmp(&right.penalty)
        .then(left.gap.cmp(&right.gap))
        .then(right.is_dir.cmp(&left.is_dir))
        .then(left.depth.cmp(&right.depth))
        .then_with(|| match (left.modified, right.modified) {
            (Some(left_modified), Some(right_modified)) => right_modified.cmp(&left_modified),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        .then_with(|| left.path.cmp(&right.path))
}
