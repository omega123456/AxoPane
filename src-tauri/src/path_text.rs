pub const VERBATIM_PREFIX: &str = r"\\?\";
pub const VERBATIM_UNC_PREFIX: &str = r"\\?\UNC\";
pub const UNC_PREFIX: &str = r"\\";
pub const WINDOWS_SEPARATOR: char = '\\';
pub const PATH_SEPARATORS: [char; 2] = ['\\', '/'];
pub const POSIX_ROOT: &str = "/";
pub const DRIVE_DESIGNATOR_LEN: usize = 2;

pub fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = strip_prefix_ignore_ascii_case(path, VERBATIM_UNC_PREFIX) {
        return format!("{UNC_PREFIX}{rest}");
    }
    path.strip_prefix(VERBATIM_PREFIX)
        .unwrap_or(path)
        .to_owned()
}

pub fn strip_unc_prefix(path: &str) -> Option<&str> {
    strip_prefix_ignore_ascii_case(path, VERBATIM_UNC_PREFIX)
        .or_else(|| path.strip_prefix(UNC_PREFIX))
}

pub fn is_drive_designator(text: &str) -> bool {
    text.len() == DRIVE_DESIGNATOR_LEN && text.ends_with(':')
}

pub fn starts_with_drive_designator(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= DRIVE_DESIGNATOR_LEN && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

pub fn strip_home_prefix(text: &str, is_separator: impl Fn(char) -> bool) -> Option<&str> {
    let rest = text.strip_prefix('~')?;
    (rest.is_empty() || rest.starts_with(is_separator)).then_some(rest)
}
