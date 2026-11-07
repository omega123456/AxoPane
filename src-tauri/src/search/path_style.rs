//! Host-independent path handling for search. Everything reports Windows
//! paths and the macOS index POSIX ones; parsing them by an explicit style
//! (instead of `std::path`) keeps both testable on every OS.

use crate::path_text::{
    starts_with_drive_designator, strip_verbatim_prefix, DRIVE_DESIGNATOR_LEN, UNC_PREFIX,
};

/// A UNC root splits into server, share and the remaining path.
const UNC_ROOT_PARTS: usize = 3;

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

    pub fn is_separator(self, character: char) -> bool {
        match self {
            Self::Windows => character == '\\' || character == '/',
            Self::Posix => character == '/',
        }
    }

    /// Drops the Windows verbatim prefixes `\\?\` and `\\?\UNC\`.
    pub fn strip_verbatim(self, path: &str) -> String {
        if self == Self::Posix {
            return path.to_owned();
        }
        strip_verbatim_prefix(path)
    }

    pub fn split<'a>(self, path: &'a str) -> PathParts<'a> {
        let (root, rest) = self.split_root(path);
        PathParts {
            root,
            components: rest
                .split(|character| self.is_separator(character))
                .filter(|part| !part.is_empty())
                .collect(),
        }
    }

    /// Appends `name` to `base`, adding a separator unless `base` already
    /// ends with one (as roots do).
    pub fn join(self, base: &str, name: &str) -> String {
        if base.is_empty() || base.ends_with(|character| self.is_separator(character)) {
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
    if starts_with_drive_designator(path) {
        let rest = path[DRIVE_DESIGNATOR_LEN..].trim_start_matches(['\\', '/']);
        return (format!("{}\\", &path[..DRIVE_DESIGNATOR_LEN]), rest);
    }
    let unc = path
        .strip_prefix(UNC_PREFIX)
        .or_else(|| path.strip_prefix("//"));
    if let Some(unc) = unc {
        let mut parts = unc.splitn(UNC_ROOT_PARTS, ['\\', '/']);
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
