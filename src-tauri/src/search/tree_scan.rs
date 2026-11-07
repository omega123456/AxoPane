//! Collects folder candidates from the macOS folder-size index tree. The
//! tree is read in chunks of arena slots, so a search never holds the
//! index's read lock long enough to stall its writer. The tree stores
//! folders only, and paths are built as POSIX strings directly (the tree's
//! own `PathBuf` joins would follow the host OS).

use std::path::{Path, PathBuf};

use super::matcher::{nfc, FoldedName, NameKind};
use super::path_style::PathStyle;
use super::query::{ParsedQuery, Segment};
use super::results::{rank_candidates, BROWSE_PARENT_CAP, CANDIDATE_CAP};
use super::source::FolderTreeSource;
use super::{Candidate, CandidateBatch, Control, SearchError, SearchOptions, SearchScope};
use crate::size::index::tree::FolderSizeTree;

/// Arena slots examined per read-lock hold.
pub const SLOTS_PER_READ: usize = 32_768;
/// Matches accumulate to twice the cap before being pruned to the best.
const FOUND_PRUNE_THRESHOLD: usize = 2 * CANDIDATE_CAP;

pub fn candidates(
    source: &dyn FolderTreeSource,
    query: &ParsedQuery,
    options: &SearchOptions,
    control: &Control<'_>,
) -> Result<CandidateBatch, SearchError> {
    candidates_in_chunks(source, query, options, control, SLOTS_PER_READ)
}

/// [`candidates`] with an explicit number of slots per read.
pub fn candidates_in_chunks(
    source: &dyn FolderTreeSource,
    query: &ParsedQuery,
    options: &SearchOptions,
    control: &Control<'_>,
    slots_per_read: usize,
) -> Result<CandidateBatch, SearchError> {
    if options.scope == SearchScope::Files {
        return Ok(CandidateBatch::default());
    }
    match (&query.name_seg, &query.anchor) {
        // Anchored name searches scan too; ranking keeps them inside the
        // anchor, which bounds every lock hold.
        (Some(name), _) => scan(source, name, options, control, slots_per_read.max(1)),
        (None, Some(anchor)) => list_children(source, &[anchor.path(PathStyle::Posix)], options),
        (None, None) if query.browse => {
            let parent_query = query.browse_parents();
            let parents =
                candidates_in_chunks(source, &parent_query, options, control, slots_per_read)?;
            let top: Vec<String> =
                rank_candidates(parents.candidates, &parent_query, PathStyle::Posix)
                    .into_iter()
                    .take(BROWSE_PARENT_CAP)
                    .map(|hit| hit.path)
                    .collect();
            list_children(source, &top, options)
        }
        (None, None) => Ok(CandidateBatch::default()),
    }
}

/// Folders whose own name matches `name`. Past the cap, the best-tier
/// matches are kept, so exact and prefix hits survive a broad query.
fn scan(
    source: &dyn FolderTreeSource,
    name: &Segment,
    options: &SearchOptions,
    control: &Control<'_>,
    slots_per_read: usize,
) -> Result<CandidateBatch, SearchError> {
    let mut found: Vec<(u8, Candidate)> = Vec::new();
    let mut truncated = false;
    let mut start = 0;
    loop {
        if !control.proceed()? {
            truncated = true;
            break;
        }
        let mut end = start;
        let mut slots = 0;
        source.read(&mut |tree, blocked| {
            let blocked = blocked_prefixes(blocked);
            slots = tree.slot_len();
            end = (start + slots_per_read).min(slots);
            for id in start..end {
                let Some((node_name, _)) = tree.live_node(id as u32) else {
                    continue;
                };
                if !name.may_match(node_name) {
                    continue;
                }
                let Some(hit) = name.matches(&FoldedName::new(&nfc(node_name)), NameKind::Folder)
                else {
                    continue;
                };
                if let Some(path) = visible_path(tree, id as u32, &blocked, options.include_hidden)
                {
                    found.push((hit.tier, folder(path)));
                }
            }
        })?;
        if found.len() > FOUND_PRUNE_THRESHOLD {
            keep_best(&mut found);
            truncated = true;
        }
        if end >= slots {
            break;
        }
        start = end;
    }
    if found.len() > CANDIDATE_CAP {
        keep_best(&mut found);
        truncated = true;
    }
    Ok(CandidateBatch {
        candidates: found.into_iter().map(|(_, candidate)| candidate).collect(),
        truncated,
    })
}

fn keep_best(found: &mut Vec<(u8, Candidate)>) {
    found.sort_by_key(|(tier, _)| *tier);
    found.truncate(CANDIDATE_CAP);
}

/// The direct child folders of `parents`, in one read.
fn list_children(
    source: &dyn FolderTreeSource,
    parents: &[String],
    options: &SearchOptions,
) -> Result<CandidateBatch, SearchError> {
    let mut batch = CandidateBatch::default();
    source.read(&mut |tree, blocked| {
        let blocked = blocked_prefixes(blocked);
        for parent in parents {
            let Some(id) = tree.lookup(Path::new(parent)) else {
                continue;
            };
            for &child in tree.child_ids(id) {
                if batch.candidates.len() == CANDIDATE_CAP {
                    batch.truncated = true;
                    return;
                }
                if let Some(path) = visible_path(tree, child, &blocked, options.include_hidden) {
                    batch.candidates.push(folder(path));
                }
            }
        }
    })?;
    Ok(batch)
}

/// The node's POSIX path, unless it is hidden (a dot-named component while
/// hidden items are off) or inside a blocked protected root.
fn visible_path(
    tree: &FolderSizeTree,
    id: u32,
    blocked: &[String],
    include_hidden: bool,
) -> Option<String> {
    let mut names = Vec::new();
    let mut cursor = Some(id);
    while let Some(current) = cursor {
        let (name, parent) = tree.live_node(current)?;
        names.push(name);
        cursor = parent;
    }
    // Leaf first; the last entry is the root, which holds a full path.
    let (root, rest) = names.split_last()?;
    let components: Vec<&str> = rest.iter().rev().copied().collect();
    if !include_hidden && components.iter().any(|name| name.starts_with('.')) {
        return None;
    }
    let path = PathStyle::Posix.compose(root, &components);
    if blocked
        .iter()
        .any(|prefix| path.starts_with(prefix.as_str()))
    {
        return None;
    }
    Some(path)
}

/// `root/` prefixes: a protected root stays visible, its contents do not.
fn blocked_prefixes(blocked: &[PathBuf]) -> Vec<String> {
    blocked
        .iter()
        .map(|root| format!("{}/", root.to_string_lossy().trim_end_matches('/')))
        .collect()
}

fn folder(path: String) -> Candidate {
    Candidate {
        path,
        is_dir: true,
        modified: None,
    }
}
