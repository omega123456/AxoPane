pub mod fake_provider;
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub mod macos;
pub mod provider;
pub mod shell_executor;
pub mod token_store;
pub mod types;
#[cfg(any(feature = "test-utils", not(target_os = "windows")))]
pub mod unsupported;
#[cfg(target_os = "windows")]
pub mod windows;

use std::sync::Arc;

use provider::{dedupe_provider_items, NativeMenuProvider, ProviderNativeMenuItem};
use shell_executor::ShellExecutor;
use token_store::NativeMenuTokenStore;
use types::{
    InvokeNativeMenuRequest, LoadNativeMenuRequest, LoadNativeMenuResponse, NativeMenuItem,
};

use crate::ipc::types::{MenuActionStatus, OpenWithRequest, ShowPropertiesRequest};

pub struct NativeMenuService {
    provider: Arc<dyn NativeMenuProvider>,
    executor: ShellExecutor,
    token_store: NativeMenuTokenStore,
}

impl Default for NativeMenuService {
    fn default() -> Self {
        Self::new(default_provider())
    }
}

impl NativeMenuService {
    pub fn new(provider: Arc<dyn NativeMenuProvider>) -> Self {
        Self {
            provider,
            executor: ShellExecutor::default(),
            token_store: NativeMenuTokenStore::default(),
        }
    }

    pub fn load_menu(&self, request: LoadNativeMenuRequest) -> LoadNativeMenuResponse {
        self.token_store.clear_all();
        self.token_store.replace_request(&request.request_id);
        let items = dedupe_provider_items(self.provider.load_menu(&request, &self.executor));

        LoadNativeMenuResponse {
            request_id: request.request_id.clone(),
            items: items
                .into_iter()
                .map(|item| self.normalize_item(&request.request_id, item))
                .collect(),
        }
    }

    pub fn invoke_menu_action(&self, request: InvokeNativeMenuRequest) -> MenuActionStatus {
        let Some(stored) = self.token_store.take(&request.token) else {
            log::warn!(
                "native menu invocation rejected for stale or unknown token: {}",
                request.token
            );
            return MenuActionStatus::unsupported("stale-or-unknown-token");
        };

        self.provider.invoke(&stored.invocation, &self.executor)
    }

    pub fn show_properties(&self, request: ShowPropertiesRequest) -> MenuActionStatus {
        #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
        {
            return windows::show_properties(&request);
        }

        #[cfg(any(feature = "test-utils", not(target_os = "windows")))]
        {
            unsupported::show_properties(&request.paths)
        }
    }

    pub fn open_with(&self, request: OpenWithRequest) -> MenuActionStatus {
        #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
        {
            return windows::open_with(&request);
        }

        #[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
        {
            return macos::open_with(&request.path);
        }

        #[cfg(any(
            feature = "test-utils",
            all(not(target_os = "windows"), not(target_os = "macos"))
        ))]
        {
            unsupported::open_with(&request.path)
        }
    }

    pub fn execution_count(&self) -> u64 {
        self.executor.execution_count()
    }

    fn normalize_item(&self, request_id: &str, item: ProviderNativeMenuItem) -> NativeMenuItem {
        let invoke_token = item
            .invocation
            .map(|invocation| self.token_store.issue_token(request_id, invocation));

        NativeMenuItem {
            id: item.id,
            label: item.label,
            enabled: item.enabled,
            danger: item.danger,
            canonical_action_kind: item.canonical_action_kind,
            normalized_verb: item.normalized_verb,
            invoke_token,
            icon: item.icon,
            children: item
                .children
                .into_iter()
                .map(|child| self.normalize_item(request_id, child))
                .collect(),
        }
    }
}

pub fn default_provider() -> Arc<dyn NativeMenuProvider> {
    #[cfg(feature = "test-utils")]
    {
        Arc::new(fake_provider::FakeNativeMenuProvider)
    }

    #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
    {
        Arc::new(windows::WindowsNativeMenuProvider)
    }

    #[cfg(all(not(feature = "test-utils"), not(target_os = "windows")))]
    {
        Arc::new(unsupported::UnsupportedNativeMenuProvider)
    }
}
