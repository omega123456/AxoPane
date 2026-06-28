use super::provider::{NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem};
use super::shell_executor::ShellExecutor;
use super::types::LoadNativeMenuRequest;
use crate::ipc::types::MenuActionStatus;

const UNSUPPORTED_MESSAGE: &str = "unsupported";

fn unsupported_status() -> MenuActionStatus {
    MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE)
}

#[derive(Default)]
pub struct UnsupportedNativeMenuProvider;

impl NativeMenuProvider for UnsupportedNativeMenuProvider {
    fn load_menu(
        &self,
        _request: &LoadNativeMenuRequest,
        _executor: &ShellExecutor,
    ) -> Vec<ProviderNativeMenuItem> {
        Vec::new()
    }

    fn invoke(
        &self,
        _invocation: &ProviderInvocation,
        _executor: &ShellExecutor,
    ) -> MenuActionStatus {
        unsupported_status()
    }
}

pub fn show_properties(_paths: &[String]) -> MenuActionStatus {
    unsupported_status()
}

pub fn open_with(_path: &str) -> MenuActionStatus {
    unsupported_status()
}
