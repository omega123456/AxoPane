use std::borrow::Cow;

use file_explorer_lib::search::matcher::{
    match_token, match_words, merge_ranges, nfc, FoldedName, Match, NameKind, Pattern, Token,
    CONTAINS, EXACT, FUZZY, FUZZY_MIN_CHARS, PREFIX,
};

fn token(word: &str, name: &str, kind: NameKind) -> Option<Match> {
    match_token(&Token::new(word), &FoldedName::new(&nfc(name)), kind)
}

fn tier(word: &str, name: &str, kind: NameKind) -> Option<u8> {
    token(word, name, kind).map(|found| found.tier)
}

const FILE: NameKind = NameKind::File;
const FOLDER: NameKind = NameKind::Folder;

#[test]
fn nfc_borrows_normalized_text_and_composes_decomposed_text() {
    assert!(matches!(nfc("report"), Cow::Borrowed("report")));
    assert!(matches!(nfc("café"), Cow::Borrowed(_)));
    assert_eq!(nfc("cafe\u{301}"), "café");
    assert_eq!(tier("café", "Cafe\u{301}", FOLDER), Some(EXACT));
}

#[test]
fn literals_rank_exact_prefix_contains_then_fuzzy() {
    assert_eq!(tier("Invoice", "invoice", FOLDER), Some(EXACT));
    assert_eq!(tier("invoice", "Invoice.pdf", FILE), Some(EXACT));
    assert_eq!(tier("invoice", "Invoices", FOLDER), Some(PREFIX));
    assert_eq!(tier("invoice", "Old Invoices", FOLDER), Some(CONTAINS));
    assert_eq!(tier("invoce", "invoice-0412.pdf", FILE), Some(FUZZY));
    assert_eq!(tier("xyz", "invoice", FILE), None);
}

#[test]
fn only_files_drop_their_extension_and_dotfiles_keep_their_name() {
    assert_eq!(tier("invoice", "invoice.d", FOLDER), Some(PREFIX));
    assert_eq!(tier("bashrc", ".bashrc", FILE), Some(CONTAINS));
    assert_eq!(tier(".bashrc", ".bashrc", FILE), Some(EXACT));
    assert_eq!(tier("archive.tar", "archive.tar.gz", FILE), Some(EXACT));
}

#[test]
fn fuzzy_needs_three_characters_and_picks_the_tightest_window() {
    assert_eq!(FUZZY_MIN_CHARS, 3);
    assert_eq!(tier("iv", "invoice", FILE), None);
    let found = token("abc", "a_xabc!", FILE).expect("contains");
    assert_eq!(found.tier, CONTAINS);
    let fuzzy = token("abc", "a_xab-c", FILE).expect("fuzzy");
    assert_eq!(fuzzy.tier, FUZZY);
    // a(3) b(4) c(6), not the first `a` at 0.
    assert_eq!(fuzzy.ranges, vec![(3, 5), (6, 7)]);
    assert_eq!(token("", "anything", FILE), None);
}

#[test]
fn globs_match_the_whole_name_as_exact_and_highlight_literals() {
    let found = token("inv*.pdf", "Invoice-0412.PDF", FILE).expect("glob");
    assert_eq!(found.tier, EXACT);
    assert_eq!(found.ranges, vec![(0, 3), (12, 16)]);
    assert_eq!(tier("*.pdf", "notes.txt", FILE), None);
    assert_eq!(tier("?nv*", "invoice", FILE), Some(EXACT));
    assert_eq!(tier("*ab", "aab", FILE), Some(EXACT));
    assert_eq!(tier("a*", "", FILE), None);
    assert_eq!(tier("inv**", "inv", FILE), Some(EXACT));
    assert_eq!(tier("in?", "invoice", FILE), None);
}

#[test]
fn ranges_are_utf16_offsets_into_the_display_name() {
    assert_eq!(
        token("report", "😀 report", FILE).expect("hit").ranges,
        vec![(3, 9)]
    );
    assert_eq!(
        token("報告", "報告書", FOLDER).expect("hit").ranges,
        vec![(0, 2)]
    );
    // İ keeps its form instead of expanding to two chars when folded.
    assert_eq!(tier("İstanbul", "İSTANBUL", FOLDER), Some(EXACT));
    assert_eq!(
        token("stanbul", "İstanbul", FOLDER).expect("hit").ranges,
        vec![(1, 8)]
    );
}

#[test]
fn multi_word_segments_try_the_phrase_then_every_word() {
    let name = FoldedName::new("Q3 report.docx");
    let phrase = Pattern::new("q3 report");
    let words = [Token::new("q3"), Token::new("report")];
    let exact = match_words(Some(&phrase), &words, &name, FILE).expect("phrase");
    assert_eq!(exact.tier, EXACT);

    let swapped = [Token::new("report"), Token::new("q3")];
    let anded = match_words(Some(&Pattern::new("report q3")), &swapped, &name, FILE).expect("and");
    assert_eq!(anded.tier, CONTAINS);
    assert_eq!(anded.ranges, vec![(0, 2), (3, 9)]);

    let fuzzy_word = [Token::new("q3"), Token::new("rpt")];
    assert_eq!(
        match_words(None, &fuzzy_word, &name, FILE).map(|found| found.tier),
        Some(FUZZY)
    );
    let missing = [Token::new("q3"), Token::new("zzz")];
    assert_eq!(match_words(None, &missing, &name, FILE), None);
    let single = [Token::new("report")];
    assert_eq!(
        match_words(None, &single, &name, FILE).map(|found| found.tier),
        Some(CONTAINS)
    );
}

#[test]
fn patterns_expose_their_folded_text() {
    let pattern = Pattern::new("ALEX");
    assert_eq!(pattern.len(), 4);
    assert!(!pattern.is_empty() && Pattern::new("").is_empty());
    assert_eq!(pattern.as_string(), "alex");
    assert!(pattern.equals(&FoldedName::new("Alex")));
    assert!(!pattern.equals(&FoldedName::new("Alexios")));
    assert!(matches!(Token::new("a?c"), Token::Glob(_)));
    assert!(matches!(Token::new("abc"), Token::Literal(_)));
}

#[test]
fn merged_ranges_join_overlaps_and_neighbours() {
    assert_eq!(
        merge_ranges(vec![(5, 7), (0, 2), (2, 3), (6, 9)]),
        vec![(0, 3), (5, 9)]
    );
    assert_eq!(merge_ranges(Vec::new()), Vec::<(u32, u32)>::new());
}

#[test]
fn name_kind_of_entry_maps_directories_to_folders() {
    assert_eq!(NameKind::of_entry(true), FOLDER);
    assert_eq!(NameKind::of_entry(false), FILE);
}
