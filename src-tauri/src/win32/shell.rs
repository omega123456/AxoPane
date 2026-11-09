use std::ffi::OsStr;

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::wide_nul;

#[derive(Default)]
pub struct ShellExecuteRequest<'a> {
    pub verb: Option<&'a OsStr>,
    pub file: &'a OsStr,
    pub parameters: Option<&'a OsStr>,
    pub directory: Option<&'a OsStr>,
}

pub fn shell_execute(request: &ShellExecuteRequest<'_>) -> isize {
    let verb = request.verb.map(wide_nul);
    let file = wide_nul(request.file);
    let parameters = request.parameters.map(wide_nul);
    let directory = request.directory.map(wide_nul);
    let result = unsafe {
        ShellExecuteW(
            None,
            optional_pcwstr(verb.as_deref()),
            PCWSTR(file.as_ptr()),
            optional_pcwstr(parameters.as_deref()),
            optional_pcwstr(directory.as_deref()),
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize
}

fn optional_pcwstr(value: Option<&[u16]>) -> PCWSTR {
    value.map_or(PCWSTR::null(), |wide| PCWSTR(wide.as_ptr()))
}
