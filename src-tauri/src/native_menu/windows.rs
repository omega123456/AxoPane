#[cfg(not(feature = "test-utils"))]
use super::helper_protocol::{HelperOperation, HelperResult};
#[cfg(not(feature = "test-utils"))]
use super::helper_supervisor::{HelperCall, HelperRole, DISCOVERY_DEADLINE, INVOCATION_DEADLINE};
use super::provider::{
    NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem, RoleMenuRequest,
};
use super::shell_executor::ShellExecutor;
use super::types::LoadNativeMenuRequest;
use crate::ipc::types::{MenuActionStatus, OpenWithRequest, ShowPropertiesRequest};

const UNSUPPORTED_MESSAGE: &str = "unsupported";

#[derive(Default)]
pub struct WindowsNativeMenuProvider;

impl NativeMenuProvider for WindowsNativeMenuProvider {
    fn load_menu(
        &self,
        request: &LoadNativeMenuRequest,
        executor: &ShellExecutor,
    ) -> Vec<ProviderNativeMenuItem> {
        #[cfg(not(feature = "test-utils"))]
        {
            let _ = executor;
            let call = HelperCall {
                operation: HelperOperation::Discover(request.clone()),
                deadline: DISCOVERY_DEADLINE,
            };
            match super::helper_supervisor::shared().call(HelperRole::Interactive, call) {
                Ok(HelperResult::Items(items)) => items,
                _ => Vec::new(),
            }
        }
        #[cfg(feature = "test-utils")]
        {
            let request = request.clone();
            executor.execute(move || load_menu_impl(&request))
        }
    }

    fn invoke(
        &self,
        invocation: &ProviderInvocation,
        executor: &ShellExecutor,
    ) -> MenuActionStatus {
        #[cfg(not(feature = "test-utils"))]
        {
            let _ = executor;
            let call = HelperCall {
                operation: HelperOperation::Invoke(invocation.clone()),
                deadline: INVOCATION_DEADLINE,
            };
            match super::helper_supervisor::shared().call(HelperRole::Interactive, call) {
                Ok(HelperResult::Status(status)) => status,
                _ => MenuActionStatus::unsupported("native-helper-failed"),
            }
        }
        #[cfg(feature = "test-utils")]
        {
            let invocation = invocation.clone();
            executor.execute(move || invoke_impl(invocation))
        }
    }

    fn load_menu_for_role(
        &self,
        role_request: RoleMenuRequest<'_>,
        executor: &ShellExecutor,
    ) -> Vec<ProviderNativeMenuItem> {
        #[cfg(not(feature = "test-utils"))]
        {
            let _ = executor;
            let call = HelperCall {
                operation: HelperOperation::Discover(role_request.request.clone()),
                deadline: DISCOVERY_DEADLINE,
            };
            match super::helper_supervisor::shared().call(role_request.role, call) {
                Ok(HelperResult::Items(items)) => items,
                _ => Vec::new(),
            }
        }
        #[cfg(feature = "test-utils")]
        {
            self.load_menu(role_request.request, executor)
        }
    }
}

pub fn show_properties(request: &ShowPropertiesRequest) -> MenuActionStatus {
    #[cfg(not(feature = "test-utils"))]
    {
        real::show_properties_impl(request)
    }

    #[cfg(feature = "test-utils")]
    {
        let _ = request;
        MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE)
    }
}

pub fn open_with(request: &OpenWithRequest) -> MenuActionStatus {
    #[cfg(not(feature = "test-utils"))]
    {
        real::open_with_impl(request)
    }

    #[cfg(feature = "test-utils")]
    {
        let _ = request;
        MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE)
    }
}

#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
pub(crate) fn helper_load_menu(request: &LoadNativeMenuRequest) -> Vec<ProviderNativeMenuItem> {
    load_menu_impl(request)
}

#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
pub(crate) fn helper_invoke(invocation: ProviderInvocation) -> MenuActionStatus {
    invoke_impl(invocation)
}

#[cfg(feature = "test-utils")]
fn load_menu_impl(_request: &LoadNativeMenuRequest) -> Vec<ProviderNativeMenuItem> {
    Vec::new()
}

#[cfg(feature = "test-utils")]
fn invoke_impl(_invocation: ProviderInvocation) -> MenuActionStatus {
    MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE)
}

#[cfg(not(feature = "test-utils"))]
fn load_menu_impl(request: &LoadNativeMenuRequest) -> Vec<ProviderNativeMenuItem> {
    real::load_menu_impl(request)
}

#[cfg(not(feature = "test-utils"))]
fn invoke_impl(invocation: ProviderInvocation) -> MenuActionStatus {
    real::invoke_impl(invocation)
}

#[cfg(not(feature = "test-utils"))]
#[path = "windows/real.rs"]
mod real;
