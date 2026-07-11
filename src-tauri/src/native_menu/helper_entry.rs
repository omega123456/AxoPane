//! Child-process entry point. It stays alive for one supervised role; a poisoned
//! shell extension still dies with this process, never the UI process.

use std::io::{self, BufReader, BufWriter, Read, Write};

use super::helper_protocol::{
    read_frame, write_frame, HelperOperation, HelperRequest, HelperResponse, HelperResult,
};

#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

pub const HELPER_ARGUMENT: &str = "--axopane-native-menu-helper";

#[cfg(not(feature = "test-utils"))]
pub fn try_run_from_args() -> bool {
    if !std::env::args_os().any(|argument| argument == HELPER_ARGUMENT) {
        return false;
    }
    #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
    let initialized_com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
    #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
    if !initialized_com {
        return true;
    }

    let _ = run_framed_stdio(io::stdin().lock(), io::stdout().lock());

    #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
    unsafe {
        CoUninitialize();
    }
    true
}

pub fn run_framed_stdio(input: impl Read, output: impl Write) -> io::Result<()> {
    let mut input = BufReader::new(input);
    let mut output = BufWriter::new(output);
    loop {
        let request: HelperRequest = match read_frame(&mut input) {
            Ok(request) => request,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        let result = dispatch(request.operation);
        write_frame(
            &mut output,
            &HelperResponse {
                request_id: request.request_id,
                result,
            },
        )?;
    }
}

fn dispatch(operation: HelperOperation) -> HelperResult {
    #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
    {
        match operation {
            HelperOperation::Discover(request) => {
                HelperResult::Items(super::windows::helper_load_menu(&request))
            }
            HelperOperation::Invoke(invocation) => {
                HelperResult::Status(super::windows::helper_invoke(invocation))
            }
            HelperOperation::Properties(request) => {
                HelperResult::Status(super::windows::show_properties(&request))
            }
            HelperOperation::OpenWith(request) => {
                HelperResult::Status(super::windows::open_with(&request))
            }
        }
    }
    #[cfg(any(not(target_os = "windows"), feature = "test-utils"))]
    {
        let _ = operation;
        HelperResult::Status(crate::ipc::types::MenuActionStatus::unsupported(
            "unsupported",
        ))
    }
}

#[cfg(feature = "test-utils")]
pub fn dispatch_for_tests(operation: HelperOperation) -> HelperResult {
    dispatch(operation)
}
