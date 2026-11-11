use std::path::Path;

use file_explorer_lib::search::file_scan::{scan_files, FileScan, FileSelection};
use file_explorer_lib::search::matcher::{nfc, FoldedName, NameKind};
use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::{ParsedQuery, Segment};
use file_explorer_lib::search::rank::Scored;
use file_explorer_lib::search::results::{rank_candidates, CandidatePool, BROWSE_PARENT_CAP};
use file_explorer_lib::search::tree_scan::TreeSearchRun;
use file_explorer_lib::search::{Candidate, Control, SearchOptions, SearchScope};
use file_explorer_lib::size::index::names::signature::{signature_of, SignedName};
use file_explorer_lib::size::index::tree::DirectoryListing;

use crate::search_file_scan_integration::{folder, packed, Fixture, Folder, SMALL_BLOCK_TARGET};
use crate::search_tree_scan_integration::{never, parsed, run, Source};

const HIDDEN_PREFIX: char = '.';
const KELVIN_SIGN_NAME: &str = "\u{212A}elvin.txt";
const NFD_RESUME_NAME: &str = "re\u{301}sume\u{301}.txt";
const NFD_CAFE_FOLDER: &str = "/Users/alex/Cafe\u{301}";
const ARCHIVE: &str = "/Users/alex/archive";
const DOT_FOLDER_GRAFT: &str = "/Users/alex/.config/tool";
const OVERLAY_FOLDER: &str = "/Users/alex/project/docs";
const HEAP_FOLDER: &str = "/Users/alex/project/drafts";
const HIDDEN_HEAP_FOLDER: &str = "/Users/alex/project/.agent/scratch";

const EVERY_OPTION: [SearchOptions; 6] = [
    options(true, SearchScope::All),
    options(false, SearchScope::All),
    options(true, SearchScope::Files),
    options(false, SearchScope::Files),
    options(true, SearchScope::Folders),
    options(false, SearchScope::Folders),
];

const fn options(include_hidden: bool, scope: SearchScope) -> SearchOptions {
    SearchOptions {
        include_hidden,
        scope,
    }
}

fn base_library() -> Vec<Folder> {
    vec![
        folder(
            "/Users/alex/project",
            &["README.md", "ReadMe.TXT", "Makefile", ".env", "notes v1.md"],
        ),
        folder(
            OVERLAY_FOLDER,
            &["guide.md", "report-2026_v1.2.txt", "a+b=c.md", "#todo!.md"],
        ),
        folder(
            "/Users/alex/project/.git/objects",
            &["pack-01.idx", "readme-object"],
        ),
        folder(
            "/Users/alex/project/.agent/plans",
            &["plan.md", "readme-plan.md"],
        ),
        folder(
            "/Users/alex/Résumés",
            &["résumé.pdf", NFD_RESUME_NAME, KELVIN_SIGN_NAME, "RESUME.md"],
        ),
        folder(NFD_CAFE_FOLDER, &["menu.md", "Café-readme.md"]),
        folder("/Users/alex/archive/box-a", &["record-a.md", "readme-a.md"]),
        folder("/Users/alex/archive/box-b", &["record-b.md"]),
        folder(DOT_FOLDER_GRAFT, &["config.json", "readme.txt"]),
        folder("/Applications", &["Calendar.app", "Kelvin.app"]),
    ]
}

fn archive_rewalk() -> Vec<Folder> {
    vec![
        folder(ARCHIVE, &["index.md"]),
        folder(
            "/Users/alex/archive/box-a",
            &["record-a2.md", "readme-new.md"],
        ),
        folder("/Users/alex/archive/box-c", &["Record-C.MD", "notes-c.txt"]),
    ]
}

fn dot_folder_rewalk() -> Vec<Folder> {
    vec![
        folder(DOT_FOLDER_GRAFT, &["grafted-readme.md"]),
        folder(
            "/Users/alex/.config/tool/inner",
            &["inner-readme.md", "Tool.cfg"],
        ),
    ]
}

struct Equivalence {
    fixture: Fixture,
}

impl Equivalence {
    fn new() -> Self {
        let fixture = Fixture::with_block_target(&base_library(), SMALL_BLOCK_TARGET);
        fixture.graft(ARCHIVE, &archive_rewalk());
        fixture.graft(DOT_FOLDER_GRAFT, &dot_folder_rewalk());
        let equivalence = Self { fixture };
        equivalence.set_overlay_names(OVERLAY_FOLDER, &["guide.md", "fresh-readme.md"]);
        equivalence.add_heap_folder(HEAP_FOLDER, &["draft-readme.md", "Ünïcödé.md"]);
        equivalence.add_heap_folder(HIDDEN_HEAP_FOLDER, &["scratch-readme.md"]);
        equivalence
    }

    fn source(&self) -> &Source {
        &self.fixture.source
    }

    fn set_overlay_names(&self, path: &str, files: &[&str]) {
        let slot = self.fixture.slot(path);
        let mut names = self.source().names.lock().unwrap();
        names.set_names(slot, packed(files));
    }

    fn add_heap_folder(&self, path: &str, files: &[&str]) {
        let mut tree = self.source().tree.lock().unwrap();
        let listing = DirectoryListing::<&str>::scanned(0, &[]);
        let slot = tree
            .merge_record_slot(Path::new(path), listing)
            .expect("heap folder added");
        let mut names = self.source().names.lock().unwrap();
        names.set_names(slot, packed(files));
    }

    fn indexed_folders(&self) -> Vec<IndexedFolder> {
        let tree = self.source().tree.lock().unwrap();
        let names = self.source().names.lock().unwrap();
        let mut folders = Vec::new();
        let mut lineage: Vec<String> = Vec::new();
        let mut cursor = tree.depth_first();
        while let Some(visited) = tree.next_folder(&mut cursor) {
            lineage.truncate(visited.depth);
            lineage.push(visited.name.to_owned());
            let packed_names = names
                .names_of(visited.slot)
                .expect("names")
                .unwrap_or_default();
            folders.push(IndexedFolder {
                lineage: lineage.clone(),
                files: packed_names
                    .names()
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .collect(),
            });
        }
        folders
    }

    fn assert_matches_reference(&self, queries: &[&str]) {
        let index = self.indexed_folders();
        let cases = queries.iter().flat_map(|raw| {
            EVERY_OPTION
                .iter()
                .map(move |options| QueryCase { raw, options })
        });
        for case in cases {
            self.assert_case_matches_reference(&index, case);
        }
    }

    fn assert_case_matches_reference(&self, index: &[IndexedFolder], case: QueryCase<'_>) {
        let QueryCase { raw, options } = case;
        let actual = run(self.source(), raw, options).candidates;
        let reference = Reference { index, options };
        let expected = reference.candidates(&parsed(raw));
        assert_eq!(sorted(&actual), sorted(&expected), "{raw:?} {options:?}");
        assert_eq!(
            ranked(actual, raw),
            ranked(expected, raw),
            "{raw:?} {options:?}"
        );
    }
}

impl Equivalence {
    fn assert_file_tiers_match_reference(&self, queries: &[&str]) {
        let index = self.indexed_folders();
        let cases = queries.iter().flat_map(|raw| {
            EVERY_OPTION
                .iter()
                .map(move |options| QueryCase { raw, options })
        });
        for case in cases {
            let QueryCase { raw, options } = case;
            let query = parsed(raw);
            let segment = query.name_seg.as_ref().expect("name segment");
            let reference = Reference {
                index: &index,
                options,
            };
            assert_eq!(
                self.file_tiers(segment, options),
                reference.file_tiers(segment),
                "{raw:?} {options:?}"
            );
        }
    }

    fn file_tiers(&self, segment: &Segment, options: &SearchOptions) -> Vec<(String, u8)> {
        let control = Control {
            cancelled: &never,
            expired: &never,
        };
        let run = TreeSearchRun {
            source: self.source(),
            options,
            control: &control,
        };
        let scan = FileScan {
            selection: FileSelection::Everywhere,
            name: Some(segment),
        };
        let mut pool = CandidatePool::default();
        scan_files(&run, scan, &mut pool).expect("file scan");
        let found = pool.into_kept().items;
        sorted_tiers(found.into_iter().map(|hit| (hit.item.path, hit.tier)))
    }
}

fn sorted_tiers(tiers: impl Iterator<Item = (String, u8)>) -> Vec<(String, u8)> {
    let mut sorted: Vec<(String, u8)> = tiers.collect();
    sorted.sort_unstable();
    sorted
}

#[derive(Clone, Copy)]
struct QueryCase<'case> {
    raw: &'case str,
    options: &'case SearchOptions,
}

struct IndexedFolder {
    lineage: Vec<String>,
    files: Vec<String>,
}

impl IndexedFolder {
    fn path(&self) -> String {
        let (root, components) = self.lineage.split_first().expect("a rooted folder");
        let names: Vec<&str> = components.iter().map(String::as_str).collect();
        PathStyle::Posix.compose(root, &names)
    }

    fn name(&self) -> &str {
        self.lineage.last().expect("a named folder")
    }

    fn is_shown(&self, include_hidden: bool) -> bool {
        if include_hidden {
            return true;
        }
        let below_root = &self.lineage[1..];
        let has_hidden_component = below_root.iter().any(|name| is_hidden(name));
        !has_hidden_component
    }

    fn lies_in(&self, ancestor: &IndexedFolder) -> bool {
        self.lineage.starts_with(&ancestor.lineage)
    }

    fn is_child_of(&self, parent: &IndexedFolder) -> bool {
        let one_level_deeper = self.lineage.len() == parent.lineage.len() + 1;
        one_level_deeper && self.lies_in(parent)
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with(HIDDEN_PREFIX)
}

fn reference_tier(segment: &Segment, name: &str, kind: NameKind) -> Option<u8> {
    let found = segment.matches(&FoldedName::new(&nfc(name)), kind)?;
    Some(found.tier)
}

fn reference_match(segment: &Segment, name: &str, kind: NameKind) -> bool {
    reference_tier(segment, name, kind).is_some()
}

fn folder_candidate(folder: &IndexedFolder) -> Candidate {
    Candidate {
        path: folder.path(),
        is_dir: true,
        modified: None,
    }
}

fn file_candidate(folder: &IndexedFolder, file: &str) -> Candidate {
    Candidate {
        path: PathStyle::Posix.join(&folder.path(), file),
        is_dir: false,
        modified: None,
    }
}

struct Reference<'index> {
    index: &'index [IndexedFolder],
    options: &'index SearchOptions,
}

impl Reference<'_> {
    fn candidates(&self, query: &ParsedQuery) -> Vec<Candidate> {
        if let Some(segment) = &query.name_seg {
            return self.named(query, segment);
        }
        if let Some(anchor) = &query.anchor {
            return self.listed_in(&[anchor.path(PathStyle::Posix)]);
        }
        self.listed_in(&self.browsed_parents(query))
    }

    fn lists_folders(&self) -> bool {
        self.options.scope != SearchScope::Files
    }

    fn lists_files(&self) -> bool {
        self.options.scope != SearchScope::Folders
    }

    fn shown_folders(&self) -> impl Iterator<Item = &IndexedFolder> {
        let include_hidden = self.options.include_hidden;
        self.index
            .iter()
            .filter(move |folder| folder.is_shown(include_hidden))
    }

    fn shows_file(&self, file: &str) -> bool {
        if self.options.include_hidden {
            return true;
        }
        !is_hidden(file)
    }

    fn named(&self, query: &ParsedQuery, segment: &Segment) -> Vec<Candidate> {
        let mut found = Vec::new();
        if self.lists_folders() {
            let matching = self
                .shown_folders()
                .filter(|folder| reference_match(segment, folder.name(), NameKind::Folder));
            found.extend(matching.map(folder_candidate));
        }
        if self.lists_files() {
            found.extend(self.named_files(query, segment));
        }
        found
    }

    fn named_files(&self, query: &ParsedQuery, segment: &Segment) -> Vec<Candidate> {
        let roots = self.file_roots(query);
        let selected = self
            .shown_folders()
            .filter(|folder| roots.iter().any(|root| folder.lies_in(root)));
        let mut found = Vec::new();
        for folder in selected {
            let matching = folder
                .files
                .iter()
                .filter(|file| self.finds_file(segment, file));
            found.extend(matching.map(|file| file_candidate(folder, file)));
        }
        found
    }

    fn file_tiers(&self, segment: &Segment) -> Vec<(String, u8)> {
        let mut tiers = Vec::new();
        for folder in self.shown_folders() {
            tiers.extend(self.folder_file_tiers(folder, segment));
        }
        sorted_tiers(tiers.into_iter())
    }

    fn folder_file_tiers(&self, folder: &IndexedFolder, segment: &Segment) -> Vec<(String, u8)> {
        let shown_files = folder.files.iter().filter(|file| self.shows_file(file));
        let tiered = shown_files.filter_map(|file| {
            let tier = reference_tier(segment, file, NameKind::File)?;
            Some((file_candidate(folder, file).path, tier))
        });
        tiered.collect()
    }

    fn finds_file(&self, segment: &Segment, file: &str) -> bool {
        if !self.shows_file(file) {
            return false;
        }
        reference_match(segment, file, NameKind::File)
    }

    fn file_roots(&self, query: &ParsedQuery) -> Vec<&IndexedFolder> {
        if let Some(anchor) = &query.anchor {
            return self.folders_at(&[anchor.path(PathStyle::Posix)]);
        }
        let Some(root_segment) = query.folder_segs.last() else {
            return self
                .index
                .iter()
                .filter(|folder| folder.lineage.len() == 1)
                .collect();
        };
        self.index
            .iter()
            .filter(|folder| reference_match(root_segment, folder.name(), NameKind::Folder))
            .collect()
    }

    fn folders_at(&self, paths: &[String]) -> Vec<&IndexedFolder> {
        self.index
            .iter()
            .filter(|folder| paths.contains(&folder.path()))
            .collect()
    }

    fn browsed_parents(&self, query: &ParsedQuery) -> Vec<String> {
        let parent_query = query.browse_parents();
        let Some(parent_segment) = &parent_query.name_seg else {
            return Vec::new();
        };
        let parents: Vec<Candidate> = self
            .shown_folders()
            .filter(|folder| reference_match(parent_segment, folder.name(), NameKind::Folder))
            .map(folder_candidate)
            .collect();
        let ranked_parents = rank_candidates(parents, &parent_query, PathStyle::Posix);
        let best = ranked_parents.into_iter().take(BROWSE_PARENT_CAP);
        best.map(|parent| parent.path).collect()
    }

    fn listed_in(&self, parent_paths: &[String]) -> Vec<Candidate> {
        let mut found = Vec::new();
        for parent in self.folders_at(parent_paths) {
            found.extend(self.listed_children(parent));
        }
        found
    }

    fn listed_children(&self, parent: &IndexedFolder) -> Vec<Candidate> {
        let mut found = Vec::new();
        if self.lists_folders() {
            let children = self
                .shown_folders()
                .filter(|folder| folder.is_child_of(parent));
            found.extend(children.map(folder_candidate));
        }
        let lists_parent_files = self.lists_files() && parent.is_shown(self.options.include_hidden);
        if lists_parent_files {
            let files = parent.files.iter().filter(|file| self.shows_file(file));
            found.extend(files.map(|file| file_candidate(parent, file)));
        }
        found
    }
}

fn sorted(candidates: &[Candidate]) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = candidates
        .iter()
        .map(|candidate| (candidate.path.clone(), candidate.is_dir))
        .collect();
    entries.sort_unstable();
    entries
}

fn ranked(candidates: Vec<Candidate>, raw: &str) -> Vec<Scored> {
    rank_candidates(candidates, &parsed(raw), PathStyle::Posix)
}

#[test]
fn the_fixture_holds_every_class_the_prefilter_must_keep_lossless() {
    let equivalence = Equivalence::new();
    let tree = equivalence.source().tree.lock().unwrap();
    let names = equivalence.source().names.lock().unwrap();
    assert!(
        !names.dead_slots().ranges().is_empty(),
        "grafts left dead ranges"
    );
    assert!(names.block_count() > 2, "several blocks");
    let grafted_tool = tree.lookup(Path::new(DOT_FOLDER_GRAFT)).expect("graft");
    assert!(
        tree.hidden_ranges().contains(grafted_tool),
        "graft under a dot-folder"
    );
    let heap = tree.heap_ids();
    for heap_folder in [HEAP_FOLDER, HIDDEN_HEAP_FOLDER] {
        let slot = tree.lookup(Path::new(heap_folder)).expect("heap folder");
        assert!(heap.contains(&slot), "{heap_folder}");
    }
    let overlay = tree.lookup(Path::new(OVERLAY_FOLDER)).expect("overlay");
    assert!(!heap.contains(&overlay), "overlay names on a mapped folder");
    drop((tree, names));
    let index = equivalence.indexed_folders();
    let files: Vec<&str> = index
        .iter()
        .flat_map(|folder| folder.files.iter().map(String::as_str))
        .collect();
    for name in [
        KELVIN_SIGN_NAME,
        NFD_RESUME_NAME,
        "résumé.pdf",
        "fresh-readme.md",
        "Ünïcödé.md",
    ] {
        assert!(files.contains(&name), "{name}");
    }
    assert!(
        !files.contains(&"record-b.md"),
        "the replaced folder's names are dead"
    );
}

#[test]
fn single_and_multi_token_queries_match_the_reference() {
    Equivalence::new().assert_matches_reference(&[
        "readme", "README", "read me", "notes v1", "makefile", "record", "zzzqqq", "drafts",
        "scratch",
    ]);
}

#[test]
fn file_pass_tiers_match_the_reference_tiers() {
    Equivalence::new().assert_file_tiers_match_reference(&[
        "readme",
        "read me",
        "rdme",
        "*.md",
        "?eadme*",
        "résumé",
        "kelvin",
        "2026_v1.2",
        "zzzqqq",
    ]);
}

#[test]
fn glob_queries_match_the_reference() {
    Equivalence::new().assert_matches_reference(&["*.md", "?eadme*", "re*me.md", "*.MD", "???"]);
}

#[test]
fn non_ascii_digit_and_punctuation_queries_match_the_reference() {
    Equivalence::new().assert_matches_reference(&[
        "résumé",
        "resume",
        "kelvin",
        "café",
        "ünïcödé",
        "2026_v1.2",
        "a+b=c",
        "#todo",
        "-01.idx",
    ]);
}

#[test]
fn scoped_folder_segment_queries_match_the_reference() {
    Equivalence::new().assert_matches_reference(&[
        "project/*.md",
        "docs/read",
        "archive/rec",
        ".config/readme",
        "tool/*",
        "café/menu",
        "~/project/read",
    ]);
}

#[test]
fn browse_and_anchor_listings_match_the_reference() {
    Equivalence::new().assert_matches_reference(&[
        "project/",
        "alex/",
        ".agent/",
        "box-a/",
        "~/project/",
        "~/.config/tool/",
        "~/archive/box-c/",
        "~/project/drafts/",
    ]);
}

#[test]
fn signatures_reject_names_missing_a_required_character_class() {
    let query = parsed("*.rs");
    let segment = query.name_seg.as_ref().expect("name segment");
    let prefilter = segment.prefilter();
    assert!(prefilter.admits_signature(signature_of(b"main.rs")));
    assert!(prefilter.admits_signature(signature_of(b"SRC.RS")));
    assert!(!prefilter.admits_signature(signature_of(b"main_rs")));
    assert!(!prefilter.admits_signature(signature_of(b"main.r")));
    assert!(prefilter.admits_name(SignedName::of(b"lib.rs")));
    assert!(
        !prefilter.admits_name(SignedName::of(b"rs.x")),
        "letters out of order"
    );
    assert!(prefilter.admits_name(SignedName::of("ü.rs".as_bytes())));
}

#[test]
fn non_ascii_names_always_pass_and_non_ascii_queries_reject_ascii_names() {
    let accented = parsed("résumé");
    let accented = accented
        .name_seg
        .as_ref()
        .expect("name segment")
        .prefilter();
    assert!(!accented.admits_signature(signature_of(b"resume.txt")));
    assert!(accented.admits_signature(signature_of(NFD_RESUME_NAME.as_bytes())));
    assert!(
        accented.admits_name(SignedName::of(b"\xff\xfe")),
        "invalid UTF-8 reaches the matcher"
    );

    let kelvin = parsed("kelvin");
    let kelvin = kelvin.name_seg.as_ref().expect("name segment").prefilter();
    assert!(kelvin.admits_name(SignedName::of(KELVIN_SIGN_NAME.as_bytes())));
    assert!(!kelvin.admits_name(SignedName::of(b"elvin.txt")));
}
