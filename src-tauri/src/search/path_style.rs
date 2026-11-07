//! Host-independent path handling for search. Everything reports Windows
//! paths and the macOS index POSIX ones; parsing them by an explicit style
//! (instead of `std::path`) keeps both testable on every OS.

/// Which path convention a backend's paths follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    Windows,
    Posix,
}

/// A path split into its root and its named components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathParts<'a> {
    /// `C:\`, `\\server\share\` or `/`; empty for a relative path.
    pub root: String,
    pub components: Vec<&'a str>,
}

impl PathStyle {
    pub fn separator(self) -> char {
        match self {
            Self::Windows => '\\',
            Self::Posix => '/',
        }
    }

    pub fn is_separator(self, c: char) -> bool {
        match self {
            Self::Windows => c == '\\' || c == '/',
            Self::Posix => c == '/',
        }
    }

    /// Drops the Windows verbatim prefixes `\\?\` and `\\?\UNC\`.
    pub fn strip_verbatim(self, path: &str) -> String {
        if self == Self::Posix {
            return path.to_owned();
        }
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
    }

    pub fn split<'a>(self, path: &'a str) -> PathParts<'a> {
        let (root, rest) = self.split_root(path);
        PathParts {
            root,
            components: rest
                .split(|c| self.is_separator(c))
                .filter(|part| !part.is_empty())
                .collect(),
        }
    }

    /// Appends `name` to `base`, adding a separator unless `base` already
    /// ends with one (as roots do).
    pub fn join(self, base: &str, name: &str) -> String {
        if base.is_empty() || base.ends_with(|c| self.is_separator(c)) {
            format!("{base}{name}")
        } else {
            format!("{base}{}{name}", self.separator())
        }
    }

    /// Rebuilds a path from a root and components with this style's
    /// separator.
    pub fn compose(self, root: &str, components: &[&str]) -> String {
        components
            .iter()
            .fold(root.to_owned(), |path, name| self.join(&path, name))
    }

    /// The containing folder, or `None` for a root or a bare name.
    pub fn parent(self, path: &str) -> Option<String> {
        let parts = self.split(path);
        let (_, ancestors) = parts.components.split_last()?;
        if parts.root.is_empty() && ancestors.is_empty() {
            return None;
        }
        Some(self.compose(&parts.root, ancestors))
    }

    pub fn is_root(self, path: &str) -> bool {
        let parts = self.split(path);
        !parts.root.is_empty() && parts.components.is_empty()
    }

    fn split_root(self, path: &str) -> (String, &str) {
        match self {
            Self::Posix => match path.strip_prefix('/') {
                Some(rest) => ("/".to_owned(), rest),
                None => (String::new(), path),
            },
            Self::Windows => windows_root(path),
        }
    }
}

fn windows_root(path: &str) -> (String, &str) {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let rest = path[2..].trim_start_matches(['\\', '/']);
        return (format!("{}\\", &path[..2]), rest);
    }
    let unc = path.strip_prefix(r"\\").or_else(|| path.strip_prefix("//"));
    if let Some(unc) = unc {
        let mut parts = unc.splitn(3, ['\\', '/']);
        let server = parts.next().unwrap_or_default();
        let share = parts.next().unwrap_or_default();
        if !server.is_empty() && !share.is_empty() {
            return (
                format!(r"\\{server}\{share}\"),
                parts.next().unwrap_or_default(),
            );
        }
    }
    (String::new(), path)
}
