//! Shared low-level Windows shell helpers used by both the classic
//! `IContextMenu` path (`windows.rs`) and the Windows 11 modern
//! `IExplorerCommand` path (`windows_modern.rs`).
//!
//! Everything here is real shell/COM/GDI code, so the module only compiles for
//! the production Windows build. Under `feature = "test-utils"` the providers
//! short-circuit before any of this is reached (see `windows.rs` /
//! `windows_modern.rs`), matching the machine-global-API rule in CLAUDE.md.
#![cfg(all(not(feature = "test-utils"), target_os = "windows"))]

use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use crate::native_menu::types::LoadNativeMenuRequest;

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{ILFree, SHDefExtractIconW, SHParseDisplayName};
use windows::Win32::UI::WindowsAndMessaging::HICON;

use crate::win32::gdi::{hicon_to_data_url, IconBitmapFallback, IconGuard};
use crate::win32::wide_nul;

/// Large-icon size requested from `SHDefExtractIconW`: crisp for a menu glyph.
const MENU_ICON_EXTRACT_SIZE: u32 = 32;

/// Parent directory of a path, used to bind to the containing shell folder.
pub(crate) fn parent_directory(path: &str) -> Option<PathBuf> {
    Path::new(path).parent().map(Path::to_path_buf)
}

/// True when the request targets empty folder background rather than items.
pub(crate) fn is_background_target(request: &LoadNativeMenuRequest) -> bool {
    matches!(
        request.target_kind,
        crate::native_menu::types::NativeMenuTargetKind::Background
    )
}

/// The selection the menu should act on: explicit `selected_paths` when present,
/// otherwise the single `target_path`.
pub(crate) fn selected_target_paths(request: &LoadNativeMenuRequest) -> Vec<String> {
    if !request.selected_paths.is_empty() {
        return request.selected_paths.clone();
    }

    request.target_path.iter().cloned().collect()
}

/// Removes the accelerator (`&`) markers and trailing shortcut text Windows
/// stores in menu strings, leaving a clean display label.
pub(crate) fn sanitize_menu_label(raw: String) -> String {
    let without_shortcut = raw.split('\t').next().unwrap_or_default();
    let mut label = String::with_capacity(without_shortcut.len());
    let mut chars = without_shortcut.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '&' {
            if matches!(chars.peek(), Some('&')) {
                label.push('&');
                chars.next();
            }
            continue;
        }
        label.push(character);
    }
    label.trim().to_string()
}

/// An absolute PIDL parsed from a filesystem path, freed on drop.
pub(crate) struct OwnedPidl(*mut ITEMIDLIST);

impl OwnedPidl {
    pub(crate) fn from_path(path: &str) -> windows::core::Result<Self> {
        let wide = wide_nul(path);
        let mut pidl = null_mut();
        unsafe {
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)?;
        }
        Ok(Self(pidl))
    }

    pub(crate) fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0
    }
}

impl Drop for OwnedPidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                ILFree(Some(self.0.cast_const()));
            }
        }
    }
}

/// Extracts an icon from an `IExplorerCommand::GetIcon` resource reference
/// (e.g. `"C:\\path\\app.dll,-123"` or a bare image path) and renders it to a
/// PNG `data:` URL. Returns `None` on any failure so callers degrade to a label
/// without an icon.
pub(crate) fn icon_resource_to_data_url(resource: &str) -> Option<String> {
    let trimmed = resource.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (module, index) = split_icon_resource(trimmed);
    let module_wide = wide_nul(module);
    let mut large: HICON = HICON(null_mut());
    let extracted = unsafe {
        SHDefExtractIconW(
            PCWSTR(module_wide.as_ptr()),
            index,
            0,
            Some(&mut large),
            None,
            MENU_ICON_EXTRACT_SIZE,
        )
    };
    if extracted.is_err() || large.0.is_null() {
        return None;
    }
    let _icon_guard = IconGuard(large);
    hicon_to_data_url(large, IconBitmapFallback::None)
}

fn split_icon_resource(resource: &str) -> (&str, i32) {
    match resource.rsplit_once(',') {
        Some((module, index)) => {
            let parsed = index.trim().parse::<i32>().unwrap_or(0);
            (module.trim(), parsed)
        }
        None => (resource, 0),
    }
}
