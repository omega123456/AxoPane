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
    let ancestors: Vec<String> = raw_ancestors.iter().map(|a| nfc(a).into_owned()).collect();
    let mut penalty = name_match.as_ref().map_or(0, |m| u32::from(m.tier));
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
        for (j, segment) in query.folder_segs.iter().enumerate().rev() {
            // Listing children: the innermost folder must be the parent.
            let lowest = if query.browse && j + 1 == query.folder_segs.len() {
                upper.checked_sub(1)?
            } else {
                0
            };
            let mut best: Option<(usize, super::matcher::Match)> = None;
            for i in (lowest..upper).rev() {
                let Some(found) =
                    segment.matches(&FoldedName::new(&ancestors[i]), NameKind::Folder)
                else {
                    continue;
                };
                if best.as_ref().is_none_or(|(_, b)| found.tier < b.tier) {
                    let exact = found.tier == super::matcher::EXACT;
                    best = Some((i, found));
                    if exact {
                        break;
                    }
                }
            }
            let (i, found) = best?;
            gap += (upper - 1 - i) as u32;
            penalty += u32::from(found.tier);
            hits.push((i, found.ranges));
            upper = i;
        }
    }
    let (parent, offsets) = display_parent(style, &parts.root, &ancestors);
    let mut parent_ranges: Vec<(u32, u32)> = hits
        .into_iter()
        .flat_map(|(i, ranges)| {
            let base = offsets[i];
            ranges.into_iter().map(move |(s, e)| (s + base, e + base))
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
        name_ranges: name_match.map(|m| m.ranges).unwrap_or_default(),
        parent_ranges,
    })
}

/// The parent path for display, plus the UTF-16 offset of each ancestor
/// name within it.
fn display_parent(style: PathStyle, root: &str, ancestors: &[String]) -> (String, Vec<u32>) {
    let mut text = nfc(root).into_owned();
    let mut offsets = Vec::with_capacity(ancestors.len());
    for name in ancestors {
        if !text.is_empty() && !text.ends_with(|c| style.is_separator(c)) {
            text.push(style.separator());
        }
        offsets.push(text.encode_utf16().count() as u32);
        text.push_str(name);
    }
    (text, offsets)
}

/// A total order over hits; a missing modified time sorts after known ones.
pub fn compare(a: &Scored, b: &Scored) -> Ordering {
    a.penalty
        .cmp(&b.penalty)
        .then(a.gap.cmp(&b.gap))
        .then(b.is_dir.cmp(&a.is_dir))
        .then(a.depth.cmp(&b.depth))
        .then_with(|| match (a.modified, b.modified) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.path.cmp(&b.path))
}
