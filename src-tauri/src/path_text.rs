//! Platform-neutral helpers over path text, shared by every module that has
//! to recognise Windows verbatim, UNC and drive-letter spellings.

pub const VERBATIM_PREFIX: &str = r"\\?\";
pub const VERBATIM_UNC_PREFIX: &str = r"\\?\UNC\";
pub const UNC_PREFIX: &str = r"\\";
/// Length of a drive designator such as `C:`.
pub const DRIVE_DESIGNATOR_LEN: usize = 2;

/// Rewrites `\\?\UNC\server\share` to `\\server\share` and drops a plain
/// `\\?\` prefix. The `UNC` token is matched case-insensitively, as Windows
/// does; any other path is returned unchanged.
pub fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = strip_prefix_ignore_ascii_case(path, VERBATIM_UNC_PREFIX) {
        return format!("{UNC_PREFIX}{rest}");
    }
    path.strip_prefix(VERBATIM_PREFIX)
        .unwrap_or(path)
        .to_owned()
}

/// The text after a `\\?\UNC\` or `\\` prefix, or `None` when `path` is not UNC.
pub fn strip_unc_prefix(path: &str) -> Option<&str> {
    strip_prefix_ignore_ascii_case(path, VERBATIM_UNC_PREFIX)
        .or_else(|| path.strip_prefix(UNC_PREFIX))
}

/// Whether `text` is exactly a drive designator such as `C:`.
pub fn is_drive_designator(text: &str) -> bool {
    text.len() == DRIVE_DESIGNATOR_LEN && text.ends_with(':')
}

/// Whether `path` begins with a drive designator such as `C:`.
pub fn starts_with_drive_designator(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= DRIVE_DESIGNATOR_LEN && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}
