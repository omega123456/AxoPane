pub mod fake_provider;
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub mod macos;
pub mod menu_cache;
pub mod modern_match;
pub mod provider;
pub mod shell_executor;
pub mod token_store;
pub mod types;
#[cfg(any(feature = "test-utils", not(target_os = "windows")))]
pub mod unsupported;
#[cfg(not(feature = "test-utils"))]
pub mod warm_pool;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
pub mod windows_modern;
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
pub mod windows_shell;

use std::sync::Arc;

use menu_cache::MenuCache;
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
    cache: Arc<MenuCache>,
    /// A second, dedicated COM-apartment executor for background cache
    /// warming. Kept separate from `executor` so a live right-click's
    /// enumeration never queues behind in-flight warm work.
    warm_executor: ShellExecutor,
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
            cache: Arc::new(MenuCache::default()),
            warm_executor: ShellExecutor::default(),
        }
    }

    /// Returns a cheap, `Clone`-able handle that background warm workers use
    /// to populate the shared cache without borrowing the Tauri `State`. The
    /// handle shares the provider and cache with the interactive path but
    /// enumerates through the dedicated warm executor.
    pub fn warm_handle(&self) -> NativeMenuWarmHandle {
        NativeMenuWarmHandle {
            provider: Arc::clone(&self.provider),
            cache: Arc::clone(&self.cache),
            executor: self.warm_executor.clone(),
        }
    }

    pub fn load_menu(&self, request: LoadNativeMenuRequest) -> LoadNativeMenuResponse {
        self.token_store.clear_all();
        self.token_store.replace_request(&request.request_id);

        // Per-file-type, in-memory cache: the first load of a given file type
        // pays the full shell-enumeration cost; subsequent loads reuse the cached
        // structure with their paths rebound. Fresh invoke tokens are still
        // issued per request below, so cache hits never replay stale tokens.
        let items = match self.cache.get(&request) {
            Some(cached) => cached,
            None => {
                let fresh =
                    dedupe_provider_items(self.provider.load_menu(&request, &self.executor));
                self.cache.insert(&request, &fresh);
                fresh
            }
        };

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

/// Background-warming handle: shares the provider and cache with
/// [`NativeMenuService`] but carries its own dedicated warm [`ShellExecutor`]
/// so background enumeration never delays a live right-click on the
/// interactive executor. Cheap to `Clone` (every field is `Arc`-backed).
#[derive(Clone)]
pub struct NativeMenuWarmHandle {
    provider: Arc<dyn NativeMenuProvider>,
    cache: Arc<MenuCache>,
    executor: ShellExecutor,
}

impl NativeMenuWarmHandle {
    /// Cache-only warm for a single representative request: if the request's
    /// type is already cached this is a cheap no-op (`None`); otherwise it
    /// enumerates through the dedicated warm executor, dedupes, and inserts
    /// into the shared cache, returning the newly-populated cache key.
    ///
    /// This never touches the invoke-token store and never normalizes or
    /// issues tokens — it produces no render payload, only a cache entry.
    pub fn warm(&self, request: &LoadNativeMenuRequest) -> Option<String> {
        let cache_key = menu_cache::cache_key(request);
        if self.cache.contains(request) {
            log::debug!(
                "native menu warm skipped for already cached key {} (target_kind={:?}, target_path={:?})",
                cache_key,
                request.target_kind,
                request.target_path
            );
            return None;
        }

        log::debug!(
            "native menu warm loading fresh menu for key {} (target_kind={:?}, target_path={:?})",
            cache_key,
            request.target_kind,
            request.target_path
        );
        let fresh = dedupe_provider_items(self.provider.load_menu(request, &self.executor));
        self.cache.insert(request, &fresh);
        log::debug!(
            "native menu warm cached key {} with {} root items",
            cache_key,
            fresh.len()
        );
        Some(cache_key)
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
