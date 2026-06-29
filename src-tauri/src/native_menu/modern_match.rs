//! OS-independent helpers for the Windows 11 modern context-menu path
//! (`windows_modern.rs`). These are pure string/collection/path routines with
//! no COM or registry dependencies, kept here so they compile and are tested on
//! every platform (the COM code itself is Windows-only and feature-gated).

use std::collections::HashSet;
use std::path::Path;

use crate::native_menu::types::{LoadNativeMenuRequest, NativeMenuTargetKind};

/// Parses a GUID/CLSID string in any common form (`{...}`, hyphenated, or bare
/// hex) into its canonical `u128` value. Returns `None` unless exactly 32 hex
/// digits are present.
pub fn parse_guid_u128(value: &str) -> Option<u128> {
    let hex: String = value.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() != 32 {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok()
}

/// Formats a `u128` GUID as the braced, upper-case registry form
/// (`{B41DB860-64E4-11D2-9906-E49FADC173CA}`).
pub fn format_guid_braced(value: u128) -> String {
    let b = value.to_be_bytes();
    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15]
    )
}

/// True for a bare drive root such as `C:` or `C:\`.
pub fn is_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    matches!(bytes, [drive, b':'] | [drive, b':', b'\\'] if drive.is_ascii_alphabetic())
}

/// Stable id fragment for a subcommand index path (`[]` -> `"root"`).
pub fn path_key(path: &[u32]) -> String {
    if path.is_empty() {
        return "root".to_string();
    }
    path.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("-")
}

/// The (lower-cased) shell item-type tokens a single path can match against.
pub fn path_type_tokens(path: &str) -> HashSet<String> {
    let mut tokens = HashSet::new();
    tokens.insert("allfilesystemobjects".to_string());

    if is_drive_root(path) {
        tokens.insert("drive".to_string());
        tokens.insert("folder".to_string());
        return tokens;
    }

    if Path::new(path).is_dir() {
        tokens.insert("directory".to_string());
        tokens.insert("folder".to_string());
    } else {
        tokens.insert("*".to_string());
        if let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) {
            tokens.insert(format!(".{}", extension.to_lowercase()));
        }
    }
    tokens
}

/// Per selected item, the set of item-type tokens a handler may register
/// against to apply. For background targets this is a single
/// `directory\background` token set.
pub fn selection_type_tokens(request: &LoadNativeMenuRequest) -> Vec<HashSet<String>> {
    if matches!(request.target_kind, NativeMenuTargetKind::Background) {
        return vec![HashSet::from(["directory\\background".to_string()])];
    }

    selected_paths(request)
        .iter()
        .map(|path| path_type_tokens(path))
        .collect()
}

/// The selection paths a menu acts on: explicit `selected_paths`, else the
/// single `target_path`.
pub fn selected_paths(request: &LoadNativeMenuRequest) -> Vec<String> {
    if !request.selected_paths.is_empty() {
        return request.selected_paths.clone();
    }
    request.target_path.iter().cloned().collect()
}

/// A handler applies when every selected item is covered by at least one of the
/// handler's declared item types.
pub fn handler_matches(item_types: &HashSet<String>, selection: &[HashSet<String>]) -> bool {
    if selection.is_empty() {
        return false;
    }
    selection
        .iter()
        .all(|tokens| tokens.iter().any(|token| item_types.contains(token)))
}
