//! Turns a parsed query into Everything search strings. Everything only
//! narrows the candidates: its results are a superset our matcher re-checks,
//! so these strings stay loose and never depend on the user's Everything
//! settings (whole-filename wildcards, literal operators).

use super::matcher::{Token, FUZZY_MIN_CHARS};
use super::path_style::PathStyle;
use super::query::{ParsedQuery, Segment};
use super::{Candidate, SearchOptions, SearchScope};
use crate::path_text::is_drive_designator;
use crate::size::everything::{
    filetime_to_system_time, EverythingRow, EVERYTHING_BATCH_CHUNK_SIZE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EverythingPass {
    pub search: String,
    /// Run only when the strict passes found few candidates.
    pub fuzzy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Name starts with the text: covers the exact and prefix tiers, so
    /// they survive the candidate cap.
    Prefix,
    Contains,
    /// Letters in order, as a per-character wildcard.
    Fuzzy,
}

/// The passes for a name search (with optional folder segments or anchor),
/// strictest first and without duplicates.
pub fn passes(query: &ParsedQuery, options: &SearchOptions) -> Vec<EverythingPass> {
    let Some(name) = &query.name_seg else {
        return Vec::new();
    };
    let mut out: Vec<EverythingPass> = Vec::new();
    for pass in [Pass::Prefix, Pass::Contains, Pass::Fuzzy] {
        let Some(name_terms) = name_terms(name, pass) else {
            continue;
        };
        let mut terms = filters(options);
        terms.extend(name_terms);
        for segment in &query.folder_segs {
            terms.extend(folder_terms(segment, pass));
        }
        if let Some(anchor) = &query.anchor {
            let prefix = with_trailing_separator(&anchor.path(PathStyle::Windows));
            terms.push(format!("path:{}", quote(&prefix)));
        }
        let search = terms.join(" ");
        if out.iter().all(|existing| existing.search != search) {
            out.push(EverythingPass {
                search,
                fuzzy: pass == Pass::Fuzzy,
            });
        }
    }
    out
}

/// Lists the direct children of `parents`, in OR-chunks.
pub fn browse_children_queries(parents: &[String], options: &SearchOptions) -> Vec<String> {
    parents
        .chunks(EVERYTHING_BATCH_CHUNK_SIZE)
        .map(|chunk| {
            let group = chunk
                .iter()
                .flat_map(|parent| parent_terms(parent))
                .collect::<Vec<_>>()
                .join(" | ");
            let mut terms = filters(options);
            terms.push(format!("<{group}>"));
            terms.join(" ")
        })
        .collect()
}

/// A row as a candidate; rows whose UTF-16 names did not decode cleanly are
/// dropped because the path could not be opened.
pub fn row_to_candidate(row: EverythingRow) -> Option<Candidate> {
    if row.file_name.is_empty()
        || row.path.contains(char::REPLACEMENT_CHARACTER)
        || row.file_name.contains(char::REPLACEMENT_CHARACTER)
    {
        return None;
    }
    Some(Candidate {
        path: PathStyle::Windows.join(&row.path, &row.file_name),
        is_dir: row.is_folder,
        modified: row.modified_filetime.and_then(filetime_to_system_time),
    })
}

fn name_terms(segment: &Segment, pass: Pass) -> Option<Vec<String>> {
    match (pass, segment.tokens()) {
        (Pass::Prefix, [Token::Literal(pattern)]) => {
            Some(vec![format!("startwith:{}", quote(&pattern.as_string()))])
        }
        (Pass::Prefix, _) => None,
        (_, tokens) => Some(
            tokens
                .iter()
                .map(|token| quote(&token_text(token, pass)))
                .collect(),
        ),
    }
}

fn folder_terms(segment: &Segment, pass: Pass) -> Vec<String> {
    segment
        .tokens()
        .iter()
        .map(|token| match token {
            // A path-wide wildcard that needs one component to match.
            Token::Glob(pattern) => {
                format!("path:{}", quote(&format!("*\\{}\\*", pattern.as_string())))
            }
            Token::Literal(_) => format!("path:{}", quote(&token_text(token, pass))),
        })
        .collect()
}

fn token_text(token: &Token, pass: Pass) -> String {
    match token {
        Token::Literal(pattern) if pass == Pass::Fuzzy && pattern.len() >= FUZZY_MIN_CHARS => {
            let mut text = String::from("*");
            for character in pattern
                .as_string()
                .chars()
                .filter(|character| !character.is_whitespace())
            {
                text.push(character);
                text.push('*');
            }
            text
        }
        Token::Literal(pattern) | Token::Glob(pattern) => pattern.as_string(),
    }
}

fn filters(options: &SearchOptions) -> Vec<String> {
    let mut terms = Vec::new();
    match options.scope {
        SearchScope::All => {}
        SearchScope::Folders => terms.push("folder:".to_owned()),
        SearchScope::Files => terms.push("file:".to_owned()),
    }
    if !options.include_hidden {
        terms.push("!attrib:H".to_owned());
    }
    terms
}

/// Drive roots are spelled both ways: Everything reports their children's
/// parent as `C:`.
fn parent_terms(parent: &str) -> Vec<String> {
    let trimmed = parent.trim_end_matches('\\');
    if is_drive_designator(trimmed) {
        return vec![
            format!("parent:{}", quote(trimmed)),
            format!("parent:{}", quote(&format!("{trimmed}\\"))),
        ];
    }
    vec![format!("parent:{}", quote(trimmed))]
}

fn with_trailing_separator(path: &str) -> String {
    if path.ends_with('\\') {
        return path.to_owned();
    }
    format!("{path}\\")
}

/// Windows names cannot contain `"`, so dropping it is lossless.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('"', ""))
}
