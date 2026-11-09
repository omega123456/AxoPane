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

const MENU_ICON_EXTRACT_SIZE: u32 = 32;

pub(crate) fn parent_directory(path: &str) -> Option<PathBuf> {
    Path::new(path).parent().map(Path::to_path_buf)
}

pub(crate) fn is_background_target(request: &LoadNativeMenuRequest) -> bool {
    matches!(
        request.target_kind,
        crate::native_menu::types::NativeMenuTargetKind::Background
    )
}

pub(crate) fn sanitize_menu_label(raw: String) -> String {
    let without_shortcut = raw.split('\t').next().unwrap_or_default();
    let mut label = String::with_capacity(without_shortcut.len());
    let mut chars = without_shortcut.chars().peekable();
    while let Some(character) = chars.next() {
        let keeps_character = character != '&' || chars.next_if_eq(&'&').is_some();
        if keeps_character {
            label.push(character);
        }
    }
    label.trim().to_string()
}

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

pub(crate) fn icon_resource_to_data_url(resource: &str) -> Option<String> {
    let trimmed = resource.trim();
    if trimmed.is_empty() {
        return None;
    }

    let icon_resource = split_icon_resource(trimmed);
    let module_wide = wide_nul(icon_resource.module);
    let mut large: HICON = HICON(null_mut());
    let extracted = unsafe {
        SHDefExtractIconW(
            PCWSTR(module_wide.as_ptr()),
            icon_resource.index,
            0,
            Some(&mut large),
            None,
            MENU_ICON_EXTRACT_SIZE,
        )
    };
    let icon_extracted = extracted.is_ok() && !large.0.is_null();
    if !icon_extracted {
        return None;
    }
    let _icon_guard = IconGuard(large);
    hicon_to_data_url(large, IconBitmapFallback::None)
}

struct IconResource<'resource> {
    module: &'resource str,
    index: i32,
}

fn split_icon_resource(resource: &str) -> IconResource<'_> {
    match resource.rsplit_once(',') {
        Some((module, index)) => IconResource {
            module: module.trim(),
            index: index.trim().parse::<i32>().unwrap_or(0),
        },
        None => IconResource {
            module: resource,
            index: 0,
        },
    }
}
