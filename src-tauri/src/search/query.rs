//! Parses the palette's query grammar: `/` and `\` mean "inside", a
//! trailing separator lists a folder's direct children, `~/`, `/` and `C:\`
//! anchor to a real path, `*`/`?` are per-segment globs and a space means
//! AND within one name.

use super::matcher::{self, FoldedName, Match, NameKind, Pattern, Token};
use super::path_style::PathStyle;
use crate::path_text::strip_home_prefix;

/// Searches start at this many non-space characters (anchored browses
/// like `~/` are exempt).
pub const MIN_QUERY_CHARS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    TooShort,
}

/// One `/`-separated part of the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub raw: String,
    /// The whole multi-word phrase, tried before matching word by word.
    phrase: Option<Pattern>,
    tokens: Vec<Token>,
}

impl Segment {
    fn new(raw: &str) -> Self {
        let tokens: Vec<Token> = raw.split_whitespace().map(Token::new).collect();
        let phrase = (tokens.len() > 1 && !raw.contains(['*', '?'])).then(|| Pattern::new(raw));
        Self {
            raw: raw.to_owned(),
            phrase,
            tokens,
        }
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn matches(&self, name: &FoldedName, kind: NameKind) -> Option<Match> {
        matcher::match_words(self.phrase.as_ref(), &self.tokens, name, kind)
    }

    /// Cheap pre-check for large scans: `false` only when `name` cannot
    /// match. Non-ASCII names always pass, since they may need NFC.
    pub fn may_match(&self, name: &str) -> bool {
        !name.is_ascii()
            || self
                .tokens
                .iter()
                .all(|token| token.pattern().ascii_subsequence_of(name))
    }
}

/// The real folder an anchored query starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub root: String,
    pub components: Vec<String>,
    folded: Vec<Pattern>,
}

impl Anchor {
    pub fn path(&self, style: PathStyle) -> String {
        let names: Vec<&str> = self.components.iter().map(String::as_str).collect();
        style.compose(&self.root, &names)
    }

    /// Whether a path with this `root` and these (NFC) ancestor names lies
    /// inside the anchor, compared case-insensitively.
    pub fn contains(&self, root: &str, ancestors: &[&str]) -> bool {
        Pattern::new(root).equals(&FoldedName::new(&matcher::nfc(&self.root)))
            && ancestors.len() >= self.folded.len()
            && self
                .folded
                .iter()
                .zip(ancestors)
                .all(|(pattern, name)| pattern.equals(&FoldedName::new(name)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQuery {
    pub anchor: Option<Anchor>,
    /// Folder segments, outermost first.
    pub folder_segs: Vec<Segment>,
    /// The last segment, matched against the item's own name; `None` when
    /// listing a folder's children.
    pub name_seg: Option<Segment>,
    pub browse: bool,
}

impl ParsedQuery {
    /// For `alex/`: the query that finds the folders whose children to list
    /// (the last folder segment becomes the name).
    pub fn browse_parents(&self) -> ParsedQuery {
        let mut folder_segs = self.folder_segs.clone();
        let name_seg = folder_segs.pop();
        ParsedQuery {
            anchor: None,
            folder_segs,
            name_seg,
            browse: false,
        }
    }
}

pub fn parse(raw: &str, style: PathStyle, home: Option<&str>) -> Result<ParsedQuery, ParseError> {
    let typed = raw.trim();
    if typed.is_empty() {
        return Err(ParseError::Empty);
    }
    // Both separators work everywhere in queries.
    let text = match style {
        PathStyle::Posix => typed.replace('\\', "/"),
        PathStyle::Windows => typed.to_owned(),
    };
    let text = expand_home(&text, style, home);
    let browse = text.ends_with(|character| style.is_separator(character));
    let parts = style.split(&text);
    let anchored = !parts.root.is_empty();
    let short = typed
        .chars()
        .filter(|character| !character.is_whitespace())
        .count()
        < MIN_QUERY_CHARS;
    let browses_anchored_folder = anchored && browse;
    let too_short = short && !browses_anchored_folder;
    if too_short {
        return Err(ParseError::TooShort);
    }
    let mut segs: Vec<&str> = parts
        .components
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect();
    let name_seg = take_name_segment(&mut segs, browse);
    if anchored {
        return Ok(ParsedQuery {
            anchor: Some(Anchor {
                root: parts.root,
                folded: segs.iter().map(|name| Pattern::new(name)).collect(),
                components: segs.iter().map(|name| (*name).to_owned()).collect(),
            }),
            folder_segs: Vec::new(),
            name_seg,
            browse,
        });
    }
    if segs.is_empty() && name_seg.is_none() {
        return Err(ParseError::Empty);
    }
    Ok(ParsedQuery {
        anchor: None,
        folder_segs: segs.into_iter().map(Segment::new).collect(),
        name_seg,
        browse,
    })
}

fn take_name_segment(segments: &mut Vec<&str>, browse: bool) -> Option<Segment> {
    if browse {
        return None;
    }
    segments.pop().map(Segment::new)
}

fn expand_home(text: &str, style: PathStyle, home: Option<&str>) -> String {
    let Some(home) = home else {
        return text.to_owned();
    };
    let Some(rest) = strip_home_prefix(text, |character| style.is_separator(character)) else {
        return text.to_owned();
    };
    if rest.is_empty() {
        return home.to_owned();
    }
    format!(
        "{}{rest}",
        home.trim_end_matches(|character| style.is_separator(character))
    )
}
