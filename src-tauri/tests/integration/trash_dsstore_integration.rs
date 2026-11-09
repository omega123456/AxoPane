use std::path::Path;

use file_explorer_lib::trash::{parse_put_back_for_tests, resolve_original_path_for_tests};

const FIXTURE: &[u8] = include_bytes!("fixtures/trash_put_back.dsstore");

const HEADER_VERSION: u32 = 1;
const HEADER_MAGIC: &[u8] = b"Bud1";
const HEADER_BYTES: usize = 36;
const HEADER_ROOT_OFFSET_POSITION: usize = 8;
const HEADER_ROOT_SIZE_POSITION: usize = 12;
const HEADER_ROOT_OFFSET_COPY_POSITION: usize = 16;
const BLOCK_OFFSET_BASE: usize = 4;
const BLOCK_ALIGNMENT: usize = 32;
const ADDRESS_TABLE_SLOTS: usize = 256;
const LEAF_NODE_MARKER: u32 = 0;
const DSDB_BLOCK_ID: u32 = 1;
const ROOT_NODE_BLOCK_ID: u32 = 2;
const LEFT_LEAF_BLOCK_ID: u32 = 3;
const RIGHTMOST_LEAF_BLOCK_ID: u32 = 4;
const TRASHED_DIR: &str = "Users/example/Documents/";

struct TwoLevelTree {
    root_node: Vec<u8>,
    left_leaf: Vec<u8>,
    rightmost_leaf: Vec<u8>,
}

fn utf16be(text: &str) -> Vec<u8> {
    let code_units: Vec<u16> = text.encode_utf16().collect();
    let mut bytes = (code_units.len() as u32).to_be_bytes().to_vec();
    for unit in code_units {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

fn ustr_record(name: &str, struct_id: &[u8], text: &str) -> Vec<u8> {
    let mut record = utf16be(name);
    record.extend_from_slice(struct_id);
    record.extend_from_slice(b"ustr");
    record.extend_from_slice(&utf16be(text));
    record
}

fn dir_record(name: &str) -> Vec<u8> {
    ustr_record(name, b"ptbL", TRASHED_DIR)
}

fn name_record(name: &str) -> Vec<u8> {
    ustr_record(name, b"ptbN", name)
}

fn node(rightmost_child: u32, entries: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = rightmost_child.to_be_bytes().to_vec();
    bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for entry in entries {
        bytes.extend_from_slice(entry);
    }
    bytes
}

fn internal_entry(child: u32, record: Vec<u8>) -> Vec<u8> {
    let mut entry = child.to_be_bytes().to_vec();
    entry.extend(record);
    entry
}

fn two_level_tree() -> TwoLevelTree {
    TwoLevelTree {
        left_leaf: node(
            LEAF_NODE_MARKER,
            &[
                dir_record("left.txt"),
                name_record("left.txt"),
                dir_record("middle.txt"),
            ],
        ),
        root_node: node(
            RIGHTMOST_LEAF_BLOCK_ID,
            &[internal_entry(
                LEFT_LEAF_BLOCK_ID,
                name_record("middle.txt"),
            )],
        ),
        rightmost_leaf: node(
            LEAF_NODE_MARKER,
            &[dir_record("right.txt"), name_record("right.txt")],
        ),
    }
}

fn ends_on_block_boundary(file: &[u8]) -> bool {
    (file.len() - BLOCK_OFFSET_BASE).is_multiple_of(BLOCK_ALIGNMENT)
}

fn append_aligned_block(file: &mut Vec<u8>, block: &[u8]) -> u32 {
    while !ends_on_block_boundary(file) {
        file.push(0);
    }
    let offset = (file.len() - BLOCK_OFFSET_BASE) as u32;
    file.extend_from_slice(block);
    offset
}

fn root_block(block_offsets: &[u32]) -> Vec<u8> {
    let mut block = (block_offsets.len() as u32).to_be_bytes().to_vec();
    block.extend_from_slice(&0u32.to_be_bytes());
    for slot in 0..ADDRESS_TABLE_SLOTS {
        let offset = block_offsets.get(slot).copied().unwrap_or_default();
        block.extend_from_slice(&offset.to_be_bytes());
    }
    let table_of_contents_count: u32 = 1;
    block.extend_from_slice(&table_of_contents_count.to_be_bytes());
    block.push(b"DSDB".len() as u8);
    block.extend_from_slice(b"DSDB");
    block.extend_from_slice(&DSDB_BLOCK_ID.to_be_bytes());
    block
}

fn write_u32(file: &mut [u8], position: usize, value: u32) {
    file[position..position + 4].copy_from_slice(&value.to_be_bytes());
}

fn two_level_dsstore() -> Vec<u8> {
    let tree = two_level_tree();
    let mut file = vec![0u8; HEADER_BYTES];
    write_u32(&mut file, 0, HEADER_VERSION);
    file[4..8].copy_from_slice(HEADER_MAGIC);

    let dsdb_offset = append_aligned_block(&mut file, &ROOT_NODE_BLOCK_ID.to_be_bytes());
    let root_node_offset = append_aligned_block(&mut file, &tree.root_node);
    let left_leaf_offset = append_aligned_block(&mut file, &tree.left_leaf);
    let rightmost_leaf_offset = append_aligned_block(&mut file, &tree.rightmost_leaf);
    let unused_root_block_slot = 0;
    let block_offsets = [
        unused_root_block_slot,
        dsdb_offset,
        root_node_offset,
        left_leaf_offset,
        rightmost_leaf_offset,
    ];
    let root = root_block(&block_offsets);
    let root_offset = append_aligned_block(&mut file, &root);

    write_u32(&mut file, HEADER_ROOT_OFFSET_POSITION, root_offset);
    write_u32(&mut file, HEADER_ROOT_SIZE_POSITION, root.len() as u32);
    write_u32(&mut file, HEADER_ROOT_OFFSET_COPY_POSITION, root_offset);
    file
}

#[test]
fn parse_put_back_extracts_ustr_records_from_a_real_layout_fixture() {
    let map = parse_put_back_for_tests(FIXTURE);

    let report = map.get("restored-report.txt").expect("recorded entry");
    assert_eq!(report.original_dir, "Users/example/Documents/reports/");
    assert_eq!(report.original_name, "restored-report.txt");
}

#[test]
fn parse_put_back_falls_back_to_the_bookmark_blob_for_older_ptbl_encodings() {
    let map = parse_put_back_for_tests(FIXTURE);

    let legacy = map.get("legacy-notes.md").expect("recorded entry");
    assert_eq!(legacy.original_dir, "Users/example/Archive/legacy/");
    assert_eq!(legacy.original_name, "legacy-notes.md");
}

#[test]
fn parse_put_back_skips_entries_missing_a_directory_record() {
    let map = parse_put_back_for_tests(FIXTURE);

    assert!(!map.contains_key("orphan-only-name.txt"));
}

#[test]
fn parse_put_back_walks_every_subtree_of_a_multi_level_tree() {
    let map = parse_put_back_for_tests(&two_level_dsstore());

    for name in ["left.txt", "middle.txt", "right.txt"] {
        let entry = map.get(name).expect("recorded entry");
        assert_eq!(entry.original_dir, TRASHED_DIR);
        assert_eq!(entry.original_name, name);
    }
}

#[test]
fn parse_put_back_on_an_empty_buffer_returns_an_empty_map() {
    assert!(parse_put_back_for_tests(&[]).is_empty());
}

#[test]
fn parse_put_back_on_a_truncated_header_returns_an_empty_map() {
    assert!(parse_put_back_for_tests(&FIXTURE[..20]).is_empty());
}

#[test]
fn parse_put_back_on_garbage_bytes_returns_an_empty_map_without_panicking() {
    let garbage = vec![0xAAu8; 4096];
    assert!(parse_put_back_for_tests(&garbage).is_empty());
}

#[test]
fn parse_put_back_rejects_a_bad_magic() {
    let mut corrupt = FIXTURE.to_vec();
    corrupt[4..8].copy_from_slice(b"NOPE");
    assert!(parse_put_back_for_tests(&corrupt).is_empty());
}

#[test]
fn resolve_original_path_joins_home_trash_volume_root_with_the_recovered_directory() {
    let resolved = resolve_original_path_for_tests(
        Path::new("/"),
        "Users/example/Documents/reports/",
        "restored-report.txt",
    );
    assert_eq!(
        resolved,
        "/Users/example/Documents/reports/restored-report.txt"
    );
}

#[test]
fn resolve_original_path_normalizes_a_directory_without_a_trailing_slash() {
    let resolved =
        resolve_original_path_for_tests(Path::new("/"), "Users/example/Documents", "file.txt");
    assert_eq!(resolved, "/Users/example/Documents/file.txt");
}

#[test]
fn resolve_original_path_keeps_the_private_var_firmlink_form_as_is() {
    let resolved =
        resolve_original_path_for_tests(Path::new("/"), "private/var/folders/xy/T/", "scratch.tmp");
    assert_eq!(resolved, "/private/var/folders/xy/T/scratch.tmp");
}
