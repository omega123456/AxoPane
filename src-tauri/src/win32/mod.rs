#![cfg(windows)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

#[cfg(not(feature = "test-utils"))]
pub mod com;
#[cfg(not(feature = "test-utils"))]
pub mod gdi;
#[cfg(not(feature = "test-utils"))]
pub mod shell;

pub fn wide_nul(value: impl AsRef<OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// # Safety
/// `pointer` must be null or point to a NUL-terminated UTF-16 string that stays valid for the call.
pub unsafe fn string_from_wide_ptr(pointer: *const u16) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let mut length = 0;
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    let units = unsafe { std::slice::from_raw_parts(pointer, length) };
    Some(String::from_utf16_lossy(units))
}
