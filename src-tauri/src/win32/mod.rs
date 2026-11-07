//! Shared Win32 helpers: wide strings, GDI bitmap capture, ShellExecute and
//! COM apartments. Everything that touches the shell, GDI or COM is compiled
//! out under `test-utils`, matching the machine-global-API rule in CLAUDE.md.
#![cfg(windows)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

#[cfg(not(feature = "test-utils"))]
pub mod com;
#[cfg(not(feature = "test-utils"))]
pub mod gdi;
#[cfg(not(feature = "test-utils"))]
pub mod shell;

/// NUL-terminated UTF-16 suitable for `PCWSTR` parameters.
pub fn wide_nul(value: impl AsRef<OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Reads a NUL-terminated UTF-16 string, or `None` for a null pointer.
///
/// # Safety
/// `pointer` must be null or point at a readable, NUL-terminated UTF-16 buffer.
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
