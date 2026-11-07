use std::cmp::Ordering;
use std::time::{Duration, SystemTime};

use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::{parse, ParsedQuery};
use file_explorer_lib::search::rank::{compare, score, Scored};
use file_explorer_lib::search::results::rank_candidates;
use file_explorer_lib::search::Candidate;

const WIN: PathStyle = PathStyle::Windows;
const POSIX: PathStyle = PathStyle::Posix;

fn folder(path: &str) -> Candidate {
    Candidate {
        path: path.to_owned(),
        is_dir: true,
        modified: None,
    }
}

fn file(path: &str) -> Candidate {
    Candidate {
        path: path.to_owned(),
        is_dir: false,
        modified: None,
    }
}

fn q(raw: &str, style: PathStyle) -> ParsedQuery {
    parse(
        raw,
        style,
        Some(if style == WIN {
            r"C:\Users\alex"
        } else {
            "/Users/alex"
        }),
    )
    .expect("query")
}

fn ranked(raw: &str, style: PathStyle, candidates: Vec<Candidate>) -> Vec<String> {
    rank_candidates(candidates, &q(raw, style), style)
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

fn windows_library() -> Vec<Candidate> {
    vec![
        folder(r"C:\Users\alex\Desktop"),
        folder(r"C:\Users\alex\Invoices"),
        file(r"C:\Users\alex\Invoices\invoice-0398.pdf"),
        file(r"C:\Users\alex\Documents\Finance\invoice-0412.pdf"),
        file(r"C:\Users\alex\Downloads\inv-office-2026.xlsx"),
        file(r"D:\Libraries\alex\invoice.pdf"),
        file(r"C:\Users\alexios\invoice.pdf"),
        folder(r"\\nas\home\alex.k\Invoices 2026"),
        folder(r"C:\Users\Alexandra\Old Invoices"),
        folder(r"D:\Clients\A. Lexington\Invoice"),
        file(r"C:\dev\report.ts"),
    ]
}

#[test]
fn path_queries_rank_full_matches_first() {
    assert_eq!(
        ranked("alex/invoice", WIN, windows_library()),
        [
            r"D:\Libraries\alex\invoice.pdf",
            r"C:\Users\alex\Invoices",
            r"C:\Users\alexios\invoice.pdf",
            r"C:\Users\alex\Invoices\invoice-0398.pdf",
            r"C:\Users\alex\Documents\Finance\invoice-0412.pdf",
            r"\\nas\home\alex.k\Invoices 2026",
            r"D:\Clients\A. Lexington\Invoice",
            r"C:\Users\Alexandra\Old Invoices",
            r"C:\Users\alex\Downloads\inv-office-2026.xlsx",
        ]
    );
}

#[test]
fn browsing_lists_direct_children_of_matching_folders() {
    let hits = ranked("alex/", WIN, windows_library());
    assert_eq!(
        &hits[..3],
        [
            r"C:\Users\alex\Desktop",
            r"C:\Users\alex\Invoices",
            r"D:\Libraries\alex\invoice.pdf"
        ]
    );
    assert!(!hits
        .iter()
        .any(|p| p.contains("Finance") || p.contains("0398")));
    assert!(hits.contains(&r"C:\Users\alexios\invoice.pdf".to_owned()));
    // A top-level item has no parent folder to match.
    assert!(ranked("alex/", WIN, vec![folder(r"C:\alex")]).is_empty());
}

#[test]
fn alignment_prefers_better_tiers_then_the_nearest_folder() {
    let nearest = score(
        &file(r"C:\alex\alex\report.txt"),
        &q("alex/report", WIN),
        WIN,
    )
    .expect("hit");
    assert_eq!((nearest.penalty, nearest.gap), (0, 0));
    let better = score(
        &file(r"C:\alex\alexios\report.txt"),
        &q("alex/report", WIN),
        WIN,
    )
    .expect("hit");
    assert_eq!((better.penalty, better.gap), (0, 1));
    let chained = score(
        &folder(r"C:\Users\x\alex\Docs"),
        &q("users/alex/", WIN),
        WIN,
    )
    .expect("hit");
    assert_eq!((chained.penalty, chained.gap), (0, 1));
    assert!(score(
        &file(r"C:\alex\report.txt"),
        &q("users/alex/report", WIN),
        WIN
    )
    .is_none());
}

#[test]
fn anchored_queries_stay_inside_the_anchor() {
    let candidates = vec![
        folder("/Users/alex/Documents"),
        folder("/Users/alex/Documents/Invoices"),
        folder("/Users/alexios/Documents"),
        folder("/Applications"),
    ];
    assert_eq!(
        ranked("~/", POSIX, candidates.clone()),
        ["/Users/alex/Documents"]
    );
    assert_eq!(
        ranked("~/inv", POSIX, candidates),
        ["/Users/alex/Documents/Invoices"]
    );
    let deeper = score(
        &folder("/Users/alex/Documents/Invoices"),
        &q("~/inv", POSIX),
        POSIX,
    )
    .expect("hit");
    assert_eq!(deeper.gap, 1);
}

#[test]
fn hits_carry_display_paths_and_highlight_ranges() {
    let hit = score(
        &folder(r"\\?\C:\Users\alex\Invoices"),
        &q("alex/inv", WIN),
        WIN,
    )
    .expect("hit");
    assert_eq!(hit.path, r"\\?\C:\Users\alex\Invoices");
    assert_eq!(hit.name, "Invoices");
    assert_eq!(hit.parent, r"C:\Users\alex");
    assert_eq!(hit.name_ranges, vec![(0, 3)]);
    assert_eq!(hit.parent_ranges, vec![(9, 13)]);
    assert_eq!(hit.depth, 3);

    let nfd = score(
        &folder("/Users/cafe\u{301}/Recettes"),
        &q("café/rec", POSIX),
        POSIX,
    )
    .expect("hit");
    assert_eq!(nfd.parent, "/Users/café");
    assert_eq!(nfd.parent_ranges, vec![(7, 11)]);

    let relative = score(&folder(r"alex\Invoices"), &q("alex/inv", WIN), WIN).expect("hit");
    assert_eq!(relative.parent, "alex");
    assert!(score(&folder(r"C:\"), &q("abc", WIN), WIN).is_none());
    assert!(score(&file(r"C:\notes.txt"), &q("invoice", WIN), WIN).is_none());
}

fn scored(path: &str, is_dir: bool, modified: Option<u64>) -> Scored {
    let mut hit = score(
        &Candidate {
            path: path.to_owned(),
            is_dir,
            modified: modified.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s)),
        },
        &q("report", POSIX),
        POSIX,
    )
    .expect("hit");
    hit.penalty = 0;
    hit
}

#[test]
fn ties_break_on_folders_depth_recency_name_then_path() {
    let dir = scored("/a/report", true, None);
    let file_hit = scored("/a/report.txt", false, None);
    assert_eq!(compare(&dir, &file_hit), Ordering::Less);
    assert_eq!(
        compare(&scored("/report.md", false, None), &file_hit),
        Ordering::Less
    );
    let newer = scored("/a/report.md", false, Some(20));
    let older = scored("/b/report.md", false, Some(10));
    assert_eq!(compare(&newer, &older), Ordering::Less);
    assert_eq!(
        compare(&older, &scored("/c/report.md", false, None)),
        Ordering::Less
    );
    assert_eq!(
        compare(&scored("/c/report.md", false, None), &older),
        Ordering::Greater
    );
    assert_eq!(
        compare(
            &scored("/a/Report.md", false, None),
            &scored("/b/report.zz", false, None)
        ),
        Ordering::Less
    );
    assert_eq!(
        compare(
            &scored("/a/report.md", false, None),
            &scored("/b/report.md", false, None)
        ),
        Ordering::Less
    );
}
