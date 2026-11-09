use std::cmp::Ordering;
use std::ops::Range;
use std::time::SystemTime;

use super::matcher::{nfc, FoldedName, Match, NameKind, EXACT};
use super::path_style::PathStyle;
use super::query::{Anchor, ParsedQuery, Segment};
use super::Candidate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scored {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
    pub penalty: u32,
    pub gap: u32,
    pub depth: u32,
    pub name_ranges: Vec<(u32, u32)>,
    pub parent_ranges: Vec<(u32, u32)>,
}

pub fn score(candidate: &Candidate, query: &ParsedQuery, style: PathStyle) -> Option<Scored> {
    let clean = style.strip_verbatim(&candidate.path);
    let parts = style.split(&clean);
    let (raw_name, raw_ancestors) = parts.components.split_last()?;
    let kind = NameKind::of_entry(candidate.is_dir);
    let name = nfc(raw_name).into_owned();
    let name_match = match &query.name_seg {
        Some(segment) => Some(segment.matches(&FoldedName::new(&name), kind)?),
        None => None,
    };
    let ancestors: Vec<String> = raw_ancestors
        .iter()
        .map(|ancestor| nfc(ancestor).into_owned())
        .collect();
    let ancestor_path = AncestorPath {
        root: &parts.root,
        ancestors: &ancestors,
    };
    let folders = match &query.anchor {
        Some(anchor) => match_anchor(anchor, ancestor_path, query.browse)?,
        None => match_folder_segments(query, &ancestors)?,
    };
    let name_penalty = name_match
        .as_ref()
        .map_or(0, |name_match| u32::from(name_match.tier));
    let DisplayParent {
        text: parent,
        offsets,
    } = display_parent(style, ancestor_path);
    Some(Scored {
        path: candidate.path.clone(),
        name,
        parent,
        is_dir: candidate.is_dir,
        modified: candidate.modified,
        penalty: name_penalty + folders.penalty,
        gap: folders.gap,
        depth: parts.components.len() as u32,
        name_ranges: name_match
            .map(|name_match| name_match.ranges)
            .unwrap_or_default(),
        parent_ranges: parent_ranges(folders.hits, &offsets),
    })
}

#[derive(Default)]
struct FolderMatch {
    penalty: u32,
    gap: u32,
    hits: Vec<(usize, Vec<(u32, u32)>)>,
}

#[derive(Clone, Copy)]
struct AncestorPath<'path> {
    root: &'path str,
    ancestors: &'path [String],
}

fn match_anchor(anchor: &Anchor, path: AncestorPath<'_>, browse: bool) -> Option<FolderMatch> {
    let AncestorPath { root, ancestors } = path;
    let names: Vec<&str> = ancestors.iter().map(String::as_str).collect();
    if !anchor.contains(root, &names) {
        return None;
    }
    let gap = (ancestors.len() - anchor.components.len()) as u32;
    let skips_levels = browse && gap != 0;
    if skips_levels {
        return None;
    }
    Some(FolderMatch {
        gap,
        ..FolderMatch::default()
    })
}

fn lowest_searchable_ancestor(lists_parent: bool, upper: usize) -> Option<usize> {
    if lists_parent {
        return upper.checked_sub(1);
    }
    Some(0)
}

fn match_folder_segments(query: &ParsedQuery, ancestors: &[String]) -> Option<FolderMatch> {
    let mut folders = FolderMatch::default();
    let mut upper = ancestors.len();
    for (segment_index, segment) in query.folder_segs.iter().enumerate().rev() {
        let lists_parent = query.browse && segment_index + 1 == query.folder_segs.len();
        let lowest = lowest_searchable_ancestor(lists_parent, upper)?;
        let AncestorMatch {
            ancestor_index,
            found,
        } = best_ancestor_match(segment, ancestors, lowest..upper)?;
        folders.gap += (upper - 1 - ancestor_index) as u32;
        folders.penalty += u32::from(found.tier);
        folders.hits.push((ancestor_index, found.ranges));
        upper = ancestor_index;
    }
    Some(folders)
}

struct AncestorMatch {
    ancestor_index: usize,
    found: Match,
}

fn best_ancestor_match(
    segment: &Segment,
    ancestors: &[String],
    range: Range<usize>,
) -> Option<AncestorMatch> {
    let mut best: Option<AncestorMatch> = None;
    for ancestor_index in range.rev() {
        let Some(found) = segment.matches(
            &FoldedName::new(&ancestors[ancestor_index]),
            NameKind::Folder,
        ) else {
            continue;
        };
        let improves = best
            .as_ref()
            .is_none_or(|best_match| found.tier < best_match.found.tier);
        if !improves {
            continue;
        }
        let exact = found.tier == EXACT;
        best = Some(AncestorMatch {
            ancestor_index,
            found,
        });
        if exact {
            break;
        }
    }
    best
}

fn parent_ranges(hits: Vec<(usize, Vec<(u32, u32)>)>, offsets: &[u32]) -> Vec<(u32, u32)> {
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
    parent_ranges
}

struct DisplayParent {
    text: String,
    offsets: Vec<u32>,
}

fn display_parent(style: PathStyle, path: AncestorPath<'_>) -> DisplayParent {
    let AncestorPath { root, ancestors } = path;
    let mut text = nfc(root).into_owned();
    let mut offsets = Vec::with_capacity(ancestors.len());
    for name in ancestors {
        let needs_separator =
            !text.is_empty() && !text.ends_with(|character| style.is_separator(character));
        if needs_separator {
            text.push(style.separator());
        }
        offsets.push(text.encode_utf16().count() as u32);
        text.push_str(name);
    }
    DisplayParent { text, offsets }
}

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
