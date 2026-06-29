//! Windows 11 modern (`IExplorerCommand`) context-menu enumeration and
//! invocation.
//!
//! The classic `IContextMenu` path (`windows.rs`) cannot see modern shell
//! extensions — both *packaged* (sparse-MSIX, e.g. PowerToys File Locksmith,
//! WinRAR 7) and *unpackaged* (`shell\<verb>` keys with an
//! `ExplorerCommandHandler` CLSID). This module discovers those handlers,
//! enumerates them into the same [`ProviderNativeMenuItem`] shape the classic
//! path produces, and invokes them on click.
//!
//! ## Discovery (verified empirically on Windows 11)
//! * Candidate `IExplorerCommand` CLSIDs are listed in
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Cached`
//!   as value names of the form `"{CLSID} {IID} 0xFFFF"`; we keep those whose
//!   IID is [`IExplorerCommand::IID`].
//! * Packaged handlers map to a package via
//!   `HKLM\SOFTWARE\Classes\PackagedCom\ClassIndex\{CLSID}` (the single subkey
//!   is the package full name). The CLSID activates through a normal
//!   `CoCreateInstance` out-of-process surrogate.
//! * A handler's file-type association is **not** in the registry — for
//!   packaged handlers it lives in the package's `AppxManifest.xml`
//!   (`windows.fileExplorerContextMenus`), which `BUILTIN\Users` can read
//!   directly. For unpackaged handlers it is the `HKCR\<class>\shell\<verb>`
//!   location.
//!
//! The discovered map is cached for the process lifetime so each right-click
//! only instantiates the handful of handlers that match the current selection,
//! keeping enumeration well under the menu's loading budget.
#![cfg(all(not(feature = "test-utils"), target_os = "windows"))]

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::OnceLock;

use crate::native_menu::modern_match::{
    format_guid_braced, handler_matches, parse_guid_u128, path_key, selected_paths,
    selection_type_tokens,
};
use crate::native_menu::provider::{ProviderInvocation, ProviderNativeMenuItem};
use crate::native_menu::types::{
    LoadNativeMenuRequest, NativeMenuIcon, NativeMenuIconKind, NativeMenuTargetKind,
};
use crate::native_menu::windows_shell::{
    classify_canonical_action, icon_resource_to_data_url, normalize_verb, path_to_wide,
    sanitize_menu_label, OwnedPidl,
};

use windows::core::{Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Storage::Packaging::Appx::GetPackagePathByFullName;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, HKEY,
    HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IExplorerCommand, IShellItemArray, SHCreateShellItemArrayFromIDLists, ECF_HASSUBCOMMANDS,
    ECS_DISABLED, ECS_HIDDEN,
};

const MAX_SUBMENU_DEPTH: usize = 1;

/// Fixed shell classes scanned (and cached) for unpackaged `IExplorerCommand`
/// verb handlers. Extension/ProgID-specific classes are resolved per query.
const FIXED_UNPACKAGED_CLASSES: &[&str] = &[
    "*",
    "AllFilesystemObjects",
    "Directory",
    "Directory\\Background",
    "Folder",
    "Drive",
];

/// A discovered modern context-menu handler and the lower-cased shell item
/// types it applies to (e.g. `"*"`, `"directory"`, `"directory\\background"`,
/// `".zip"`).
#[derive(Clone)]
struct HandlerRegistration {
    clsid: u128,
    packaged: bool,
    item_types: HashSet<String>,
}

/// Process-lifetime cache of handlers discovered from fixed classes + packaged
/// manifests. Per-extension unpackaged handlers are resolved per query and not
/// cached here.
static HANDLER_CACHE: OnceLock<Vec<HandlerRegistration>> = OnceLock::new();

/// Enumerates modern context-menu items for the request's selection. Returns an
/// empty vec on any failure so the classic items still render.
pub fn enumerate_modern_items(request: &LoadNativeMenuRequest) -> Vec<ProviderNativeMenuItem> {
    if matches!(
        request.target_kind,
        NativeMenuTargetKind::Tree | NativeMenuTargetKind::Tab
    ) {
        return Vec::new();
    }

    let selection = selection_type_tokens(request);
    if selection.is_empty() {
        return Vec::new();
    }

    let mut handlers = cached_handlers().clone();
    handlers.extend(per_query_extension_handlers(request));

    let array = match build_item_array(request) {
        Some(array) => array,
        None => return Vec::new(),
    };

    let mut seen_clsids: HashSet<u128> = HashSet::new();
    let mut items = Vec::new();
    for handler in handlers {
        if !seen_clsids.insert(handler.clsid) {
            continue;
        }
        if !handler_matches(&handler.item_types, &selection) {
            continue;
        }
        let Some(command) = create_command(handler.clsid) else {
            continue;
        };
        if let Some(item) = build_item_for_command(
            &command,
            &array.0,
            handler.clsid,
            handler.packaged,
            request,
            Vec::new(),
            0,
        ) {
            items.push(item);
        }
    }

    items
}

/// Invokes a modern command identified by its handler CLSID and the optional
/// subcommand index path.
pub fn invoke_modern(
    clsid: u128,
    _packaged: bool,
    request: &LoadNativeMenuRequest,
    command_path: &[u32],
) -> windows::core::Result<()> {
    let array = build_item_array(request).ok_or_else(windows::core::Error::empty)?;
    let command = create_command(clsid).ok_or_else(windows::core::Error::empty)?;

    let target = resolve_subcommand(&command, &array.0, command_path)?;
    unsafe { target.Invoke(&array.0, None) }
}

/// Walks `command_path` (subcommand indices below the top-level handler) to the
/// leaf command that should be invoked.
fn resolve_subcommand(
    command: &IExplorerCommand,
    array: &IShellItemArray,
    command_path: &[u32],
) -> windows::core::Result<IExplorerCommand> {
    let mut current = command.clone();
    for target_index in command_path {
        let enumerator = unsafe { current.EnumSubCommands()? };
        let mut found = None;
        let mut index = 0u32;
        loop {
            let mut fetched = [const { None }; 1];
            let mut count = 0u32;
            let _ = unsafe { enumerator.Next(&mut fetched, Some(&mut count)) };
            if count == 0 {
                break;
            }
            let Some(child) = fetched[0].take() else {
                break;
            };
            if is_hidden(&child, array) {
                continue;
            }
            if index == *target_index {
                found = Some(child);
                break;
            }
            index += 1;
        }
        current = found.ok_or_else(windows::core::Error::empty)?;
    }
    Ok(current)
}

/// Builds a single menu item (and, to depth 1, its subcommands) from a command.
fn build_item_for_command(
    command: &IExplorerCommand,
    array: &IShellItemArray,
    clsid: u128,
    packaged: bool,
    request: &LoadNativeMenuRequest,
    command_path: Vec<u32>,
    depth: usize,
) -> Option<ProviderNativeMenuItem> {
    let state = unsafe { command.GetState(array, false) }.ok()?;
    if state & ECS_HIDDEN.0 as u32 != 0 {
        return None;
    }
    let enabled = state & ECS_DISABLED.0 as u32 == 0;

    let title = unsafe { take_pwstr(command.GetTitle(array).ok()?) };
    let label = sanitize_menu_label(title.unwrap_or_default());
    if label.is_empty() {
        return None;
    }

    let icon = unsafe { take_pwstr(command.GetIcon(array).ok()?) }
        .and_then(|resource| icon_resource_to_data_url(&resource))
        .map(|data_url| NativeMenuIcon {
            kind: NativeMenuIconKind::DataUrl,
            data_url,
            alt: None,
        });

    let has_subcommands = unsafe { command.GetFlags() }
        .map(|flags| flags & ECF_HASSUBCOMMANDS.0 as u32 != 0)
        .unwrap_or(false);

    let children = if has_subcommands && depth < MAX_SUBMENU_DEPTH {
        enumerate_subcommands(command, array, clsid, packaged, request, &command_path, depth)
    } else {
        Vec::new()
    };

    let normalized_verb = normalize_verb(&label);
    let canonical_action_kind = classify_canonical_action(Some(&normalized_verb), &label);
    let id = format!("windows-modern-{clsid:032x}-{}", path_key(&command_path));

    let invocation = if children.is_empty() {
        Some(ProviderInvocation::WindowsModern {
            clsid,
            packaged,
            request: request.clone(),
            command_path,
        })
    } else {
        None
    };

    Some(ProviderNativeMenuItem {
        id,
        label,
        enabled,
        danger: false,
        canonical_action_kind,
        normalized_verb: Some(normalized_verb),
        icon,
        invocation,
        children,
    })
}

fn enumerate_subcommands(
    command: &IExplorerCommand,
    array: &IShellItemArray,
    clsid: u128,
    packaged: bool,
    request: &LoadNativeMenuRequest,
    parent_path: &[u32],
    depth: usize,
) -> Vec<ProviderNativeMenuItem> {
    let Ok(enumerator) = (unsafe { command.EnumSubCommands() }) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    let mut index = 0u32;
    loop {
        let mut fetched = [const { None }; 1];
        let mut count = 0u32;
        let _ = unsafe { enumerator.Next(&mut fetched, Some(&mut count)) };
        if count == 0 {
            break;
        }
        let Some(child) = fetched[0].take() else {
            break;
        };

        let mut path = parent_path.to_vec();
        path.push(index);
        if let Some(item) = build_item_for_command(
            &child, array, clsid, packaged, request, path, depth + 1,
        ) {
            items.push(item);
            index += 1;
        }
    }
    items
}

fn is_hidden(command: &IExplorerCommand, array: &IShellItemArray) -> bool {
    unsafe { command.GetState(array, false) }
        .map(|state| state & ECS_HIDDEN.0 as u32 != 0)
        .unwrap_or(true)
}

// ----- shell item array ---------------------------------------------------------

/// Holds the `IShellItemArray` for a selection together with the PIDLs that
/// back it (kept alive for the duration of the array's use).
struct ShellItemArray(IShellItemArray, #[allow(dead_code)] Vec<OwnedPidl>);

fn build_item_array(request: &LoadNativeMenuRequest) -> Option<ShellItemArray> {
    let paths = if matches!(request.target_kind, NativeMenuTargetKind::Background) {
        match request.folder_path.as_deref().or(request.target_path.as_deref()) {
            Some(folder) => vec![folder.to_string()],
            None => return None,
        }
    } else {
        selected_paths(request)
    };

    if paths.is_empty() {
        return None;
    }

    let pidls: Vec<OwnedPidl> = paths
        .iter()
        .filter_map(|path| OwnedPidl::from_path(path).ok())
        .collect();
    if pidls.is_empty() {
        return None;
    }

    let pointers: Vec<*const ITEMIDLIST> = pidls.iter().map(OwnedPidl::as_ptr).collect();
    let array = unsafe { SHCreateShellItemArrayFromIDLists(&pointers) }.ok()?;
    Some(ShellItemArray(array, pidls))
}

/// Activates an `IExplorerCommand` handler via the COM SCM.
///
/// This resolves unpackaged handlers and packaged classes that also carry a
/// classic registration (e.g. WinRAR). It returns `None` (CLASSNOTREG) for
/// *sparse-packaged* handlers whose COM server runs with package identity in a
/// surrogate (e.g. PowerToys File Locksmith): those cannot be activated from an
/// unpackaged process and require the host to run with package identity. See
/// the module docs.
fn create_command(clsid: u128) -> Option<IExplorerCommand> {
    let guid = GUID::from_u128(clsid);
    unsafe { CoCreateInstance::<_, IExplorerCommand>(&guid, None, CLSCTX_ALL) }.ok()
}

// ----- discovery + cache ---------------------------------------------------------

fn cached_handlers() -> &'static Vec<HandlerRegistration> {
    HANDLER_CACHE.get_or_init(discover_handlers)
}

fn discover_handlers() -> Vec<HandlerRegistration> {
    let mut by_clsid: HashMap<u128, HandlerRegistration> = HashMap::new();

    for handler in discover_packaged_handlers() {
        merge_handler(&mut by_clsid, handler);
    }
    for handler in discover_unpackaged_handlers(FIXED_UNPACKAGED_CLASSES) {
        merge_handler(&mut by_clsid, handler);
    }

    by_clsid.into_values().collect()
}

fn merge_handler(map: &mut HashMap<u128, HandlerRegistration>, handler: HandlerRegistration) {
    map.entry(handler.clsid)
        .and_modify(|existing| {
            existing.item_types.extend(handler.item_types.iter().cloned());
            existing.packaged = existing.packaged || handler.packaged;
        })
        .or_insert(handler);
}

/// Discovers packaged `IExplorerCommand` handlers: candidate CLSIDs come from
/// the IExplorerCommand cache, are confirmed packaged via `PackagedCom`, and
/// their item types are parsed from the package manifest.
///
/// Note: sparse-packaged handlers (e.g. File Locksmith) are still listed here,
/// but `create_command` cannot activate them from an unpackaged process — see
/// the module docs.
fn discover_packaged_handlers() -> Vec<HandlerRegistration> {
    let candidates = iexplorer_command_cached_clsids();
    let mut manifest_cache: HashMap<String, HashMap<u128, HashSet<String>>> = HashMap::new();
    let mut handlers = Vec::new();

    for clsid in candidates {
        let Some(package) = packaged_class_owner(clsid) else {
            continue;
        };
        let associations = manifest_cache
            .entry(package.clone())
            .or_insert_with(|| package_associations(&package));
        if let Some(item_types) = associations.get(&clsid) {
            if !item_types.is_empty() {
                handlers.push(HandlerRegistration {
                    clsid,
                    packaged: true,
                    item_types: item_types.clone(),
                });
            }
        }
    }

    handlers
}

fn package_associations(package_full_name: &str) -> HashMap<u128, HashSet<String>> {
    match package_install_path(package_full_name) {
        Some(root) => parse_manifest_associations(&root.join("AppxManifest.xml")),
        None => HashMap::new(),
    }
}

/// Reads the value names of the IExplorerCommand cache key and returns the
/// CLSIDs whose cached interface IID is `IExplorerCommand`.
fn iexplorer_command_cached_clsids() -> Vec<u128> {
    let Some(key) = RegKey::open(
        HKEY_CURRENT_USER,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Shell Extensions\\Cached",
    ) else {
        return Vec::new();
    };

    let iexplorer_command_iid = IExplorerCommand::IID;
    let mut clsids = Vec::new();
    for name in key.value_names() {
        // Format: "{CLSID} {IID} 0xFFFF".
        let mut parts = name.split_whitespace();
        let (Some(clsid_text), Some(iid_text)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Some(clsid), Some(iid)) = (parse_guid_u128(clsid_text), parse_guid_u128(iid_text))
        else {
            continue;
        };
        if GUID::from_u128(iid) == iexplorer_command_iid {
            clsids.push(clsid);
        }
    }
    clsids
}

/// Returns the package full name that owns a packaged COM CLSID, if any.
fn packaged_class_owner(clsid: u128) -> Option<String> {
    let key = RegKey::open(
        HKEY_LOCAL_MACHINE,
        &format!(
            "SOFTWARE\\Classes\\PackagedCom\\ClassIndex\\{}",
            format_guid_braced(clsid)
        ),
    )?;
    key.subkeys().into_iter().next()
}

/// Parses a package's `windows.fileExplorerContextMenus` associations into a
/// map of handler CLSID to the (lower-cased) item types it registers for.
fn parse_manifest_associations(manifest_path: &Path) -> HashMap<u128, HashSet<String>> {
    let mut associations: HashMap<u128, HashSet<String>> = HashMap::new();

    let Ok(contents) = std::fs::read_to_string(manifest_path) else {
        return associations;
    };
    let Ok(document) = roxmltree::Document::parse(&contents) else {
        return associations;
    };

    for verb in document
        .descendants()
        .filter(|node| node.has_tag_name("Verb"))
    {
        let Some(clsid) = verb.attribute("Clsid").and_then(parse_guid_u128) else {
            continue;
        };
        let Some(item_type) = verb
            .parent()
            .filter(|parent| parent.has_tag_name("ItemType"))
            .and_then(|parent| parent.attribute("Type"))
        else {
            continue;
        };
        associations
            .entry(clsid)
            .or_default()
            .insert(item_type.to_lowercase());
    }

    associations
}

fn package_install_path(package_full_name: &str) -> Option<PathBuf> {
    let wide = path_to_wide(package_full_name);
    let mut length = 0u32;
    // First call sizes the buffer (returns ERROR_INSUFFICIENT_BUFFER).
    let _ = unsafe { GetPackagePathByFullName(PCWSTR(wide.as_ptr()), &mut length, None) };
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize];
    let result = unsafe {
        GetPackagePathByFullName(
            PCWSTR(wide.as_ptr()),
            &mut length,
            Some(PWSTR(buffer.as_mut_ptr())),
        )
    };
    if result != ERROR_SUCCESS {
        return None;
    }
    let trimmed = (length as usize).saturating_sub(1);
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..trimmed])))
}

/// Discovers unpackaged `IExplorerCommand` verb handlers registered under the
/// given shell classes (`HKCR\<class>\shell\<verb>` carrying an
/// `ExplorerCommandHandler` CLSID value).
fn discover_unpackaged_handlers(classes: &[&str]) -> Vec<HandlerRegistration> {
    let mut handlers = Vec::new();
    for class in classes {
        for clsid in explorer_command_handlers_for_class(class) {
            handlers.push(HandlerRegistration {
                clsid,
                packaged: false,
                item_types: HashSet::from([class.to_lowercase()]),
            });
        }
    }
    handlers
}

fn explorer_command_handlers_for_class(class: &str) -> Vec<u128> {
    let Some(shell_key) = RegKey::open(HKEY_CLASSES_ROOT, &format!("{class}\\shell")) else {
        return Vec::new();
    };

    let mut clsids = Vec::new();
    for verb in shell_key.subkeys() {
        let Some(verb_key) = shell_key.open_sub(&verb) else {
            continue;
        };
        if let Some(value) = verb_key.read_sz("ExplorerCommandHandler") {
            if let Some(clsid) = parse_guid_u128(&value) {
                clsids.push(clsid);
            }
        }
    }
    clsids
}

/// Resolves unpackaged handlers registered against the selection's specific
/// file extensions (and their resolved ProgIDs) at query time.
fn per_query_extension_handlers(request: &LoadNativeMenuRequest) -> Vec<HandlerRegistration> {
    if matches!(request.target_kind, NativeMenuTargetKind::Background) {
        return Vec::new();
    }

    let mut extensions: HashSet<String> = HashSet::new();
    for path in selected_paths(request) {
        if let Some(extension) = Path::new(&path).extension().and_then(|value| value.to_str()) {
            extensions.insert(format!(".{}", extension.to_lowercase()));
        }
    }

    let mut handlers = Vec::new();
    for extension in extensions {
        for clsid in explorer_command_handlers_for_class(&extension) {
            handlers.push(HandlerRegistration {
                clsid,
                packaged: false,
                item_types: HashSet::from([extension.clone()]),
            });
        }
        if let Some(progid) = class_default_value(&extension) {
            for clsid in explorer_command_handlers_for_class(&progid) {
                handlers.push(HandlerRegistration {
                    clsid,
                    packaged: false,
                    item_types: HashSet::from([extension.clone()]),
                });
            }
        }
    }
    handlers
}

fn class_default_value(class: &str) -> Option<String> {
    let key = RegKey::open(HKEY_CLASSES_ROOT, class)?;
    key.read_sz("").filter(|value| !value.is_empty())
}

unsafe fn take_pwstr(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let value = text.to_string().ok();
    CoTaskMemFree(Some(text.0 as *const c_void));
    value
}

// ----- registry RAII -------------------------------------------------------------

struct RegKey(HKEY);

impl RegKey {
    fn open(root: HKEY, sub: &str) -> Option<Self> {
        let wide = path_to_wide(sub);
        let mut handle = HKEY(null_mut());
        let result =
            unsafe { RegOpenKeyExW(root, PCWSTR(wide.as_ptr()), Some(0), KEY_READ, &mut handle) };
        if result == ERROR_SUCCESS {
            Some(Self(handle))
        } else {
            None
        }
    }

    fn open_sub(&self, sub: &str) -> Option<Self> {
        let wide = path_to_wide(sub);
        let mut handle = HKEY(null_mut());
        let result = unsafe {
            RegOpenKeyExW(self.0, PCWSTR(wide.as_ptr()), Some(0), KEY_READ, &mut handle)
        };
        if result == ERROR_SUCCESS {
            Some(Self(handle))
        } else {
            None
        }
    }

    fn subkeys(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut index = 0u32;
        loop {
            let mut buffer = [0u16; 256];
            let mut length = buffer.len() as u32;
            let result = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut length,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if result != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buffer[..length as usize]));
            index += 1;
        }
        names
    }

    fn value_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut index = 0u32;
        loop {
            let mut buffer = [0u16; 512];
            let mut length = buffer.len() as u32;
            let result = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut length,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if result != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buffer[..length as usize]));
            index += 1;
        }
        names
    }

    fn read_sz(&self, name: &str) -> Option<String> {
        let wide_name = path_to_wide(name);
        let mut value_type = REG_VALUE_TYPE(0);
        let mut size = 0u32;
        let result = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(wide_name.as_ptr()),
                None,
                Some(&mut value_type),
                None,
                Some(&mut size),
            )
        };
        if result != ERROR_SUCCESS || size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        let result = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(wide_name.as_ptr()),
                None,
                None,
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            )
        };
        if result != ERROR_SUCCESS {
            return None;
        }

        let units = (size as usize) / 2;
        let wide: Vec<u16> = buffer
            .chunks_exact(2)
            .take(units)
            .map(|chunk| u16::from_ne_bytes([chunk[0], chunk[1]]))
            .collect();
        let text = String::from_utf16_lossy(&wide);
        Some(text.trim_end_matches('\0').to_string())
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}
