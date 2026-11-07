//! Name matching for search: NFC-normalized, case-insensitive, tiered
//! exact / prefix / contains / fuzzy, plus `*` and `?` globs. Highlight
//! ranges are UTF-16 offsets into the NFC display name, ready for JS.

use std::borrow::Cow;

use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};

pub const EXACT: u8 = 0;
pub const PREFIX: u8 = 1;
pub const CONTAINS: u8 = 2;
pub const FUZZY: u8 = 3;

/// Shorter literals never match fuzzily; one or two letters would match
/// almost every name.
pub const FUZZY_MIN_CHARS: usize = 3;

/// `text` in NFC, borrowed when it already is (the common case).
pub fn nfc(text: &str) -> Cow<'_, str> {
    if text.is_ascii() || is_nfc_quick(text.chars()) == IsNormalized::Yes {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    }
}

/// Lowercases one char, keeping it as-is when lowercasing would change the
/// char count (`İ`), so folded and display indices stay one-to-one.
fn fold_char(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(folded), None) => folded,
        _ => c,
    }
}

/// A file or folder name prepared for matching.
#[derive(Debug, Clone)]
pub struct FoldedName {
    chars: Vec<char>,
    /// UTF-16 offset of each char in the display name, plus the end.
    utf16: Vec<u32>,
}

impl FoldedName {
    /// `display` must already be NFC (see [`nfc`]).
    pub fn new(display: &str) -> Self {
        let mut chars = Vec::with_capacity(display.len());
        let mut utf16 = Vec::with_capacity(display.len() + 1);
        let mut offset = 0u32;
        for c in display.chars() {
            chars.push(fold_char(c));
            utf16.push(offset);
            offset += c.len_utf16() as u32;
        }
        utf16.push(offset);
        Self { chars, utf16 }
    }

    fn range(&self, start: usize, end: usize) -> (u32, u32) {
        (self.utf16[start], self.utf16[end])
    }

    fn ranges(&self, positions: &[usize]) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for &pos in positions {
            let (start, end) = self.range(pos, pos + 1);
            match out.last_mut() {
                Some(last) if last.1 == start => last.1 = end,
                _ => out.push((start, end)),
            }
        }
        out
    }
}

/// A query fragment, NFC-normalized and folded like [`FoldedName`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    chars: Vec<char>,
}

impl Pattern {
    pub fn new(text: &str) -> Self {
        Self {
            chars: nfc(text).chars().map(fold_char).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn as_string(&self) -> String {
        self.chars.iter().collect()
    }

    /// Whether these chars (wildcards skipped) appear in order in an ASCII
    /// `name`, case-insensitively. Allocation-free pre-check for scans.
    pub fn ascii_subsequence_of(&self, name: &str) -> bool {
        let mut wanted = self.chars.iter().filter(|c| !matches!(c, '*' | '?'));
        let mut next = wanted.next();
        for byte in name.bytes() {
            match next {
                None => break,
                Some(&c) if c == char::from(byte.to_ascii_lowercase()) => next = wanted.next(),
                Some(_) => {}
            }
        }
        next.is_none()
    }

    /// Whole-name, case-insensitive equality.
    pub fn equals(&self, name: &FoldedName) -> bool {
        self.chars == name.chars
    }
}

/// One space-separated word of a query segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Literal(Pattern),
    /// Contains `*` or `?`; matches the whole name, never fuzzily.
    Glob(Pattern),
}

impl Token {
    pub fn new(word: &str) -> Self {
        if word.contains(['*', '?']) {
            Self::Glob(Pattern::new(word))
        } else {
            Self::Literal(Pattern::new(word))
        }
    }

    pub fn pattern(&self) -> &Pattern {
        match self {
            Self::Literal(pattern) | Self::Glob(pattern) => pattern,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    File,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub tier: u8,
    /// UTF-16 `[start, end)` ranges in the display name, sorted.
    pub ranges: Vec<(u32, u32)>,
}

pub fn match_token(token: &Token, name: &FoldedName, kind: NameKind) -> Option<Match> {
    match token {
        Token::Literal(pattern) => match_literal(&pattern.chars, name, kind, FUZZY),
        Token::Glob(pattern) => glob(&pattern.chars, &name.chars).map(|positions| Match {
            tier: EXACT,
            ranges: name.ranges(&positions),
        }),
    }
}

/// Matches a multi-word segment: the whole phrase first (never fuzzily),
/// otherwise every word must match and the segment takes the worst word's
/// tier, no better than contains.
pub fn match_words(
    phrase: Option<&Pattern>,
    tokens: &[Token],
    name: &FoldedName,
    kind: NameKind,
) -> Option<Match> {
    if let [token] = tokens {
        return match_token(token, name, kind);
    }
    if let Some(found) = phrase.and_then(|p| match_literal(&p.chars, name, kind, CONTAINS)) {
        return Some(found);
    }
    let mut tier = CONTAINS;
    let mut ranges = Vec::new();
    for token in tokens {
        let found = match_token(token, name, kind)?;
        tier = tier.max(found.tier);
        ranges.extend(found.ranges);
    }
    Some(Match {
        tier,
        ranges: merge_ranges(ranges),
    })
}

fn match_literal(pattern: &[char], name: &FoldedName, kind: NameKind, worst: u8) -> Option<Match> {
    let chars = &name.chars;
    if pattern.is_empty() {
        return None;
    }
    let whole = |tier| {
        Some(Match {
            tier,
            ranges: vec![name.range(0, pattern.len())],
        })
    };
    if chars.as_slice() == pattern || stem(chars, kind) == Some(pattern) {
        return whole(EXACT);
    }
    if chars.starts_with(pattern) {
        return whole(PREFIX);
    }
    if let Some(start) = chars.windows(pattern.len()).position(|w| w == pattern) {
        return Some(Match {
            tier: CONTAINS,
            ranges: vec![name.range(start, start + pattern.len())],
        });
    }
    if worst < FUZZY || pattern.len() < FUZZY_MIN_CHARS {
        return None;
    }
    subsequence(pattern, chars).map(|positions| Match {
        tier: FUZZY,
        ranges: name.ranges(&positions),
    })
}

/// A file name without its extension; dotfiles keep their full name and
/// folders never drop one.
fn stem(chars: &[char], kind: NameKind) -> Option<&[char]> {
    if kind == NameKind::Folder {
        return None;
    }
    let dot = chars.iter().rposition(|&c| c == '.').filter(|&i| i > 0)?;
    Some(&chars[..dot])
}

/// Positions of `pattern`'s chars in order within `text`, using the
/// tightest window that ends at the earliest possible end.
fn subsequence(pattern: &[char], text: &[char]) -> Option<Vec<usize>> {
    let mut next = 0;
    let mut end = 0;
    for (i, &c) in text.iter().enumerate() {
        if c == pattern[next] {
            next += 1;
            if next == pattern.len() {
                end = i;
                break;
            }
        }
    }
    if next < pattern.len() {
        return None;
    }
    let mut start = end;
    let mut remaining = pattern.len();
    for i in (0..=end).rev() {
        if text[i] == pattern[remaining - 1] {
            remaining -= 1;
            if remaining == 0 {
                start = i;
                break;
            }
        }
    }
    let mut positions = Vec::with_capacity(pattern.len());
    let mut next = 0;
    for (i, &c) in text.iter().enumerate().take(end + 1).skip(start) {
        if next < pattern.len() && c == pattern[next] {
            positions.push(i);
            next += 1;
        }
    }
    Some(positions)
}

/// Whole-name glob match; returns the positions of the literal chars.
fn glob(pattern: &[char], text: &[char]) -> Option<Vec<usize>> {
    let (mut p, mut t) = (0, 0);
    let mut positions = Vec::new();
    // After a `*`: the pattern index past it, the text index it has consumed
    // up to, and how many positions were recorded before it.
    let mut star: Option<(usize, usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            if pattern[p] != '?' {
                positions.push(t);
            }
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p + 1, t, positions.len()));
            p += 1;
        } else if let Some((after, consumed, recorded)) = star {
            p = after;
            t = consumed + 1;
            positions.truncate(recorded);
            star = Some((after, t, recorded));
        } else {
            return None;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    (p == pattern.len()).then_some(positions)
}

pub fn merge_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}
