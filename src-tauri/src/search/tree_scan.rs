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
        let chunk = SlotChunk {
            start,
            len: slots_per_read,
        };
        let read = read_slot_chunk(source, name, options, chunk, &mut found)?;
        if found.len() > FOUND_PRUNE_THRESHOLD {
            keep_best(&mut found);
            truncated = true;
        }
        if read.end >= read.slot_count {
            break;
        }
        start = read.end;
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

#[derive(Clone, Copy)]
struct SlotChunk {
    start: usize,
    len: usize,
}

struct SlotChunkRead {
    end: usize,
    slot_count: usize,
}

fn read_slot_chunk(
    source: &dyn FolderTreeSource,
    name: &Segment,
    options: &SearchOptions,
    chunk: SlotChunk,
    found: &mut Vec<(u8, Candidate)>,
) -> Result<SlotChunkRead, SearchError> {
    let mut read = SlotChunkRead {
        end: chunk.start,
        slot_count: 0,
    };
    source.read(&mut |tree, blocked| {
        let blocked = blocked_prefixes(blocked);
        read.slot_count = tree.slot_len();
        read.end = (chunk.start + chunk.len).min(read.slot_count);
        let hits = (chunk.start..read.end)
            .filter_map(|id| folder_hit(tree, id as u32, name, &blocked, options.include_hidden));
        found.extend(hits);
    })?;
    Ok(read)
}

/// The match tier and candidate for a live, visible folder whose name
/// matches `name`.
fn folder_hit(
    tree: &FolderSizeTree,
    id: u32,
    name: &Segment,
    blocked: &[String],
    include_hidden: bool,
) -> Option<(u8, Candidate)> {
    let (node_name, _) = tree.live_node(id)?;
    if !name.may_match(node_name) {
        return None;
    }
    let hit = name.matches(&FoldedName::new(&nfc(node_name)), NameKind::Folder)?;
    let path = visible_path(tree, id, blocked, include_hidden)?;
    Some((hit.tier, folder(path)))
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
        push_children_of_parents(tree, parents, &blocked, options.include_hidden, &mut batch);
    })?;
    Ok(batch)
}

fn push_children_of_parents(
    tree: &FolderSizeTree,
    parents: &[String],
    blocked: &[String],
    include_hidden: bool,
    batch: &mut CandidateBatch,
) {
    let parent_ids = parents
        .iter()
        .filter_map(|parent| tree.lookup(Path::new(parent)));
    for parent_id in parent_ids {
        let has_room = push_visible_children(tree, parent_id, blocked, include_hidden, batch);
        if !has_room {
            return;
        }
    }
}

/// Returns `false` once the batch is full.
fn push_visible_children(
    tree: &FolderSizeTree,
    parent_id: u32,
    blocked: &[String],
    include_hidden: bool,
    batch: &mut CandidateBatch,
) -> bool {
    for &child in tree.child_ids(parent_id) {
        let batch_full = batch.candidates.len() == CANDIDATE_CAP;
        if batch_full {
            batch.truncated = true;
            return false;
        }
        if let Some(path) = visible_path(tree, child, blocked, include_hidden) {
            batch.candidates.push(folder(path));
        }
    }
    true
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
    let has_hidden_component = components.iter().any(|name| name.starts_with('.'));
    let hidden_excluded = has_hidden_component && !include_hidden;
    if hidden_excluded {
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
