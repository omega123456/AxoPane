use std::borrow::Cow;

use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};

pub const EXACT: u8 = 0;
pub const PREFIX: u8 = 1;
pub const CONTAINS: u8 = 2;
pub const FUZZY: u8 = 3;

pub const FUZZY_MIN_CHARS: usize = 3;

pub fn nfc(text: &str) -> Cow<'_, str> {
    let already_normalized = text.is_ascii() || is_nfc_quick(text.chars()) == IsNormalized::Yes;
    if already_normalized {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.nfc().collect())
}

fn fold_char(character: char) -> char {
    let mut lower = character.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(folded), None) => folded,
        _ => character,
    }
}

#[derive(Debug, Clone)]
pub struct FoldedName {
    chars: Vec<char>,
    utf16: Vec<u32>,
}

impl FoldedName {
    pub fn new(display: &str) -> Self {
        let mut chars = Vec::with_capacity(display.len());
        let mut utf16 = Vec::with_capacity(display.len() + 1);
        let mut offset = 0u32;
        for character in display.chars() {
            chars.push(fold_char(character));
            utf16.push(offset);
            offset += character.len_utf16() as u32;
        }
        utf16.push(offset);
        Self { chars, utf16 }
    }

    fn range(&self, start: usize, end: usize) -> (u32, u32) {
        (self.utf16[start], self.utf16[end])
    }

    fn ranges(&self, positions: &[usize]) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for &position in positions {
            let (start, end) = self.range(position, position + 1);
            match out.last_mut() {
                Some(last) if last.1 == start => last.1 = end,
                _ => out.push((start, end)),
            }
        }
        out
    }
}

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

    pub fn ascii_subsequence_of(&self, name: &str) -> bool {
        let mut lowered_name = name
            .bytes()
            .map(|byte| char::from(byte.to_ascii_lowercase()));
        self.chars
            .iter()
            .filter(|character| !matches!(character, '*' | '?'))
            .all(|wanted| lowered_name.any(|character| character == *wanted))
    }

    pub fn equals(&self, name: &FoldedName) -> bool {
        self.chars == name.chars
    }
}

const GLOB_WILDCARDS: [char; 2] = ['*', '?'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Literal(Pattern),
    Glob(Pattern),
}

impl Token {
    pub fn new(word: &str) -> Self {
        if word.contains(GLOB_WILDCARDS) {
            return Self::Glob(Pattern::new(word));
        }
        Self::Literal(Pattern::new(word))
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

impl NameKind {
    pub fn of_entry(is_dir: bool) -> Self {
        if is_dir {
            return Self::Folder;
        }
        Self::File
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub tier: u8,
    pub ranges: Vec<(u32, u32)>,
}

pub fn match_token(token: &Token, name: &FoldedName, kind: NameKind) -> Option<Match> {
    match token {
        Token::Literal(pattern) => {
            let literal = LiteralPattern {
                chars: &pattern.chars,
                worst: FUZZY,
            };
            match_literal(literal, name, kind)
        }
        Token::Glob(pattern) => glob(&pattern.chars, &name.chars).map(|positions| Match {
            tier: EXACT,
            ranges: name.ranges(&positions),
        }),
    }
}

pub fn match_words(words: Words<'_>, name: &FoldedName, kind: NameKind) -> Option<Match> {
    let Words { phrase, tokens } = words;
    if let [token] = tokens {
        return match_token(token, name, kind);
    }
    let phrase_match = phrase.and_then(|phrase| {
        let literal = LiteralPattern {
            chars: &phrase.chars,
            worst: CONTAINS,
        };
        match_literal(literal, name, kind)
    });
    if let Some(found) = phrase_match {
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

#[derive(Clone, Copy)]
pub struct Words<'words> {
    pub phrase: Option<&'words Pattern>,
    pub tokens: &'words [Token],
}

#[derive(Clone, Copy)]
struct LiteralPattern<'pattern> {
    chars: &'pattern [char],
    worst: u8,
}

fn match_literal(literal: LiteralPattern<'_>, name: &FoldedName, kind: NameKind) -> Option<Match> {
    let LiteralPattern {
        chars: pattern,
        worst,
    } = literal;
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
    let is_exact = chars.as_slice() == pattern || stem(chars, kind) == Some(pattern);
    if is_exact {
        return whole(EXACT);
    }
    if chars.starts_with(pattern) {
        return whole(PREFIX);
    }
    if let Some(start) = chars
        .windows(pattern.len())
        .position(|window| window == pattern)
    {
        return Some(Match {
            tier: CONTAINS,
            ranges: vec![name.range(start, start + pattern.len())],
        });
    }
    let fuzzy_allowed = worst >= FUZZY && pattern.len() >= FUZZY_MIN_CHARS;
    if !fuzzy_allowed {
        return None;
    }
    subsequence(pattern, chars).map(|positions| Match {
        tier: FUZZY,
        ranges: name.ranges(&positions),
    })
}

fn stem(chars: &[char], kind: NameKind) -> Option<&[char]> {
    if kind == NameKind::Folder {
        return None;
    }
    let extension_dot = chars
        .iter()
        .rposition(|&character| character == '.')
        .filter(|&position| position > 0)?;
    Some(&chars[..extension_dot])
}

fn subsequence(pattern: &[char], text: &[char]) -> Option<Vec<usize>> {
    let end = earliest_subsequence_end(pattern, text)?;
    let start = latest_subsequence_start(pattern, text, end);
    let mut positions = Vec::with_capacity(pattern.len());
    let mut next = 0;
    for (position, &character) in text.iter().enumerate().take(end + 1).skip(start) {
        let matches_next = next < pattern.len() && character == pattern[next];
        if matches_next {
            positions.push(position);
            next += 1;
        }
    }
    Some(positions)
}

fn earliest_subsequence_end(pattern: &[char], text: &[char]) -> Option<usize> {
    let mut matched = 0;
    for (position, &character) in text.iter().enumerate() {
        if character == pattern[matched] {
            matched += 1;
        }
        if matched == pattern.len() {
            return Some(position);
        }
    }
    None
}

fn latest_subsequence_start(pattern: &[char], text: &[char], end: usize) -> usize {
    let mut remaining = pattern.len();
    for position in (0..=end).rev() {
        if text[position] == pattern[remaining - 1] {
            remaining -= 1;
        }
        if remaining == 0 {
            return position;
        }
    }
    end
}

fn glob(pattern: &[char], text: &[char]) -> Option<Vec<usize>> {
    let mut cursor = GlobCursor::new(pattern, text);
    while cursor.has_text_left() {
        cursor.step()?;
    }
    cursor.finish()
}

struct StarBacktrack {
    pattern_after_star: usize,
    text_consumed: usize,
    recorded_positions: usize,
}

struct GlobCursor<'glob> {
    pattern: &'glob [char],
    text: &'glob [char],
    pattern_index: usize,
    text_index: usize,
    positions: Vec<usize>,
    star: Option<StarBacktrack>,
}

impl<'glob> GlobCursor<'glob> {
    fn new(pattern: &'glob [char], text: &'glob [char]) -> Self {
        Self {
            pattern,
            text,
            pattern_index: 0,
            text_index: 0,
            positions: Vec::new(),
            star: None,
        }
    }

    fn has_text_left(&self) -> bool {
        self.text_index < self.text.len()
    }

    fn step(&mut self) -> Option<()> {
        let wanted = self.pattern.get(self.pattern_index).copied();
        if wanted == Some('?') {
            self.advance_both();
            return Some(());
        }
        if wanted == Some(self.text[self.text_index]) {
            self.positions.push(self.text_index);
            self.advance_both();
            return Some(());
        }
        if wanted == Some('*') {
            self.star = Some(StarBacktrack {
                pattern_after_star: self.pattern_index + 1,
                text_consumed: self.text_index,
                recorded_positions: self.positions.len(),
            });
            self.pattern_index += 1;
            return Some(());
        }
        self.backtrack_to_star()
    }

    fn advance_both(&mut self) {
        self.pattern_index += 1;
        self.text_index += 1;
    }

    fn backtrack_to_star(&mut self) -> Option<()> {
        let star = self.star.as_mut()?;
        self.pattern_index = star.pattern_after_star;
        self.text_index = star.text_consumed + 1;
        self.positions.truncate(star.recorded_positions);
        star.text_consumed = self.text_index;
        Some(())
    }

    fn at_star(&self) -> bool {
        self.pattern.get(self.pattern_index) == Some(&'*')
    }

    fn finish(mut self) -> Option<Vec<usize>> {
        while self.at_star() {
            self.pattern_index += 1;
        }
        (self.pattern_index == self.pattern.len()).then_some(self.positions)
    }
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
