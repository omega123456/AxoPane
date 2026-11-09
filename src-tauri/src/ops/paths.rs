use std::fs;
use std::path::Path;

use crate::volumes::VolumeInfo;

pub(super) fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| is_real_directory_type(metadata.file_type()))
        .unwrap_or(false)
}

pub(super) fn is_real_directory_type(file_type: fs::FileType) -> bool {
    file_type.is_dir() && !file_type.is_symlink()
}

pub(super) fn path_entry_exists(path: &Path) -> bool {
    path.exists() || fs::symlink_metadata(path).is_ok()
}

#[derive(Debug, PartialEq, Eq)]
pub struct FileNameParts {
    pub stem: String,
    pub extension: String,
}

impl FileNameParts {
    fn numbered(&self, counter: u32) -> String {
        let FileNameParts { stem, extension } = self;
        if extension.is_empty() {
            return format!("{stem} ({counter})");
        }
        format!("{stem} ({counter}).{extension}")
    }
}

pub fn unique_name(dir: &Path, name: &str) -> String {
    let name_parts = split_name(name);
    let mut counter = 1;
    loop {
        let candidate = name_parts.numbered(counter);
        let taken = dir.join(&candidate).exists();
        if !taken {
            return candidate;
        }
        counter += 1;
    }
}

pub fn split_name(name: &str) -> FileNameParts {
    match name.rfind('.') {
        Some(index) if index > 0 => FileNameParts {
            stem: name[..index].to_string(),
            extension: name[index + 1..].to_string(),
        },
        _ => FileNameParts {
            stem: name.to_string(),
            extension: String::new(),
        },
    }
}

pub fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn volume_root_for(path: &Path, volumes: &[VolumeInfo]) -> String {
    let path_text = path.to_string_lossy().to_ascii_lowercase();

    let matched = volumes
        .iter()
        .filter(|volume| {
            let mount = volume.mount_root.to_ascii_lowercase();
            path_text == mount
                || path_text.strip_prefix(&mount).is_some_and(|remainder| {
                    remainder.starts_with('\\') || remainder.starts_with('/')
                })
        })
        .max_by_key(|volume| volume.mount_root.len());

    if let Some(volume) = matched {
        return volume.mount_root.to_ascii_lowercase();
    }

    fallback_root(&path_text)
}

pub fn fallback_root(path_text: &str) -> String {
    if let Some(index) = path_text.find(':') {
        return path_text[..=index].to_string();
    }
    let trimmed = path_text.trim_start_matches('/');
    match trimmed.split('/').next() {
        Some(first) if !first.is_empty() => format!("/{first}"),
        _ => "/".to_string(),
    }
}

pub fn is_nested_copy_target(source: &Path, target: &Path) -> bool {
    let source_components = normalized_components(source);
    let target_components = normalized_components(target);

    target_components.len() > source_components.len()
        && target_components.starts_with(&source_components)
}

pub fn normalized_components(path: &Path) -> Vec<String> {
    use std::path::Component;

    let mut normalized = Vec::new();
    for component in dunce::simplified(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => push_parent_component(&mut normalized),
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str().to_string_lossy().to_ascii_lowercase());
            }
            Component::Normal(value) => normalized.push(normalized_name(value)),
        }
    }
    normalized
}

const PARENT_COMPONENT: &str = "..";

fn push_parent_component(normalized: &mut Vec<String>) {
    let can_cancel_previous = matches!(normalized.last(), Some(last) if last != PARENT_COMPONENT);
    if can_cancel_previous {
        normalized.pop();
        return;
    }
    normalized.push(PARENT_COMPONENT.to_string());
}

#[cfg(windows)]
fn normalized_name(value: &std::ffi::OsStr) -> String {
    value.to_string_lossy().to_ascii_lowercase()
}

#[cfg(not(windows))]
fn normalized_name(value: &std::ffi::OsStr) -> String {
    value.to_string_lossy().into_owned()
}
