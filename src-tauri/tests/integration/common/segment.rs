use std::path::Path;

use file_explorer_lib::size::index::names::block::decode;
use file_explorer_lib::size::index::names::codec::DecodeScratch;
use file_explorer_lib::size::index::names::names_in;
use file_explorer_lib::size::index::segment::{MappedSegment, SegmentPaths};

pub const FIRST_SEQUENCE: u64 = 1;

pub fn paths_in(directory: &Path) -> SegmentPaths {
    SegmentPaths::numbered(directory, FIRST_SEQUENCE)
}

pub fn file_names_of(segment: &MappedSegment, id: u32) -> Vec<String> {
    let names = segment.file_names();
    let block_index = names.blocks_covering(id..id + 1).start;
    let Some(stored) = names.block(block_index) else {
        return Vec::new();
    };
    let mut scratch = DecodeScratch::default();
    let decoded = decode(stored, &mut scratch).expect("names block decodes");
    let packed = decoded.names_of(id).unwrap_or_default();
    names_in(packed)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

pub fn names_to(segment: &MappedSegment, id: u32) -> Vec<String> {
    let mut names = Vec::new();
    let mut next = Some(id);
    while let Some(current) = next {
        names.push(segment.name(current).expect("name").to_string());
        next = segment.row(current).expect("row").parent;
    }
    names.reverse();
    names
}

pub fn child_names(segment: &MappedSegment, id: u32) -> Vec<String> {
    let children = segment.children(id).expect("children");
    children
        .iter()
        .map(|child| segment.name(child).expect("child name").to_string())
        .collect()
}

pub fn descend(segment: &MappedSegment, top: u32, names: &[&str]) -> Option<u32> {
    names.iter().try_fold(top, |id, name| {
        segment.child_named(id, name).expect("child lookup")
    })
}

pub fn files_in(directory: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    files
}
