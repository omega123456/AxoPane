#![cfg(all(not(feature = "test-utils"), target_os = "windows"))]

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::OnceLock;

use crate::native_menu::canonical::{classify_canonical_action, normalize_verb};
use crate::native_menu::modern_match::{
    format_guid_braced, handler_matches, parse_guid_u128, path_key, selected_paths,
    selection_type_tokens,
};
use crate::native_menu::provider::{ProviderInvocation, ProviderNativeMenuItem};
use crate::native_menu::types::{
    LoadNativeMenuRequest, NativeMenuIcon, NativeMenuIconKind, NativeMenuTargetKind,
};
use crate::native_menu::windows_shell::{
    icon_resource_to_data_url, sanitize_menu_label, OwnedPidl,
};
use crate::win32::wide_nul;

use windows::core::{Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Storage::Packaging::Appx::GetPackagePathByFullName;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, HKEY,
    HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IExplorerCommand, IShellItemArray, SHCreateShellItemArrayFromIDLists,
    ECF_HASSUBCOMMANDS, ECS_DISABLED, ECS_HIDDEN,
};

const MAX_SUBMENU_DEPTH: usize = 1;
const REGISTRY_KEY_NAME_UNITS: usize = 256;
const REGISTRY_VALUE_NAME_UNITS: usize = 512;

const FIXED_UNPACKAGED_CLASSES: &[&str] = &[
    "*",
    "AllFilesystemObjects",
    "Directory",
    "Directory\\Background",
    "Folder",
    "Drive",
];

#[derive(Clone)]
struct HandlerRegistration {
    clsid: u128,
    packaged: bool,
    item_types: HashSet<String>,
}

static HANDLER_CACHE: OnceLock<Vec<HandlerRegistration>> = OnceLock::new();

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
        let context = ModernHandlerContext {
            array: &array.items,
            clsid: handler.clsid,
            packaged: handler.packaged,
            request,
        };
        items.extend(build_item_for_command(
            &command,
            context,
            CommandPosition::top_level(),
        ));
    }

    items
}

pub fn invoke_modern(
    clsid: u128,
    request: &LoadNativeMenuRequest,
    command_path: &[u32],
) -> windows::core::Result<()> {
    let array = build_item_array(request).ok_or_else(windows::core::Error::empty)?;
    let command = create_command(clsid).ok_or_else(windows::core::Error::empty)?;

    let target = resolve_subcommand(&command, &array.items, command_path)?;
    unsafe { target.Invoke(&array.items, None) }
}

fn resolve_subcommand(
    command: &IExplorerCommand,
    array: &IShellItemArray,
    command_path: &[u32],
) -> windows::core::Result<IExplorerCommand> {
    let mut current = command.clone();
    for target_index in command_path {
        current = visible_subcommand_at(&current, array, *target_index)?;
    }
    Ok(current)
}

fn visible_subcommand_at(
    command: &IExplorerCommand,
    array: &IShellItemArray,
    target_index: u32,
) -> windows::core::Result<IExplorerCommand> {
    let enumerator = unsafe { command.EnumSubCommands()? };
    std::iter::from_fn(|| next_subcommand(&enumerator))
        .filter(|child| !is_hidden(child, array))
        .nth(target_index as usize)
        .ok_or_else(windows::core::Error::empty)
}

fn next_subcommand(enumerator: &IEnumExplorerCommand) -> Option<IExplorerCommand> {
    let mut fetched = [const { None }; 1];
    let mut count = 0u32;
    let _ = unsafe { enumerator.Next(&mut fetched, Some(&mut count)) };
    if count == 0 {
        return None;
    }
    fetched[0].take()
}

#[derive(Clone, Copy)]
struct ModernHandlerContext<'context> {
    array: &'context IShellItemArray,
    clsid: u128,
    packaged: bool,
    request: &'context LoadNativeMenuRequest,
}

struct CommandPosition {
    command_path: Vec<u32>,
    depth: usize,
}

impl CommandPosition {
    fn top_level() -> Self {
        Self {
            command_path: Vec::new(),
            depth: 0,
        }
    }

    fn child(&self, index: u32) -> Self {
        let mut command_path = self.command_path.clone();
        command_path.push(index);
        Self {
            command_path,
            depth: self.depth + 1,
        }
    }
}

fn build_item_for_command(
    command: &IExplorerCommand,
    context: ModernHandlerContext<'_>,
    position: CommandPosition,
) -> Option<ProviderNativeMenuItem> {
    let ModernHandlerContext { array, clsid, .. } = context;
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

    let children = enumerate_subcommands(command, context, &position);

    let normalized_verb = normalize_verb(&label);
    let canonical_action_kind = classify_canonical_action(Some(&normalized_verb), &label);
    let id = format!(
        "windows-modern-{clsid:032x}-{}",
        path_key(&position.command_path)
    );

    let invocation = modern_leaf_invocation(&children, context, position.command_path);

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

fn modern_leaf_invocation(
    children: &[ProviderNativeMenuItem],
    context: ModernHandlerContext<'_>,
    command_path: Vec<u32>,
) -> Option<ProviderInvocation> {
    if !children.is_empty() {
        return None;
    }
    Some(ProviderInvocation::WindowsModern {
        clsid: context.clsid,
        packaged: context.packaged,
        request: context.request.clone(),
        command_path,
    })
}

fn has_subcommands(command: &IExplorerCommand) -> bool {
    unsafe { command.GetFlags() }
        .map(|flags| flags & ECF_HASSUBCOMMANDS.0 as u32 != 0)
        .unwrap_or(false)
}

fn enumerate_subcommands(
    command: &IExplorerCommand,
    context: ModernHandlerContext<'_>,
    parent: &CommandPosition,
) -> Vec<ProviderNativeMenuItem> {
    let descends_into_subcommands = has_subcommands(command) && parent.depth < MAX_SUBMENU_DEPTH;
    if !descends_into_subcommands {
        return Vec::new();
    }
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

        if let Some(item) = build_item_for_command(&child, context, parent.child(index)) {
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

struct ShellItemArray {
    items: IShellItemArray,
    _pidls: Vec<OwnedPidl>,
}

fn menu_target_paths(request: &LoadNativeMenuRequest) -> Option<Vec<String>> {
    let is_background = matches!(request.target_kind, NativeMenuTargetKind::Background);
    if !is_background {
        return Some(selected_paths(request));
    }
    let folder = request
        .folder_path
        .as_deref()
        .or(request.target_path.as_deref())?;
    Some(vec![folder.to_string()])
}

fn build_item_array(request: &LoadNativeMenuRequest) -> Option<ShellItemArray> {
    let paths = menu_target_paths(request)?;

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
    Some(ShellItemArray {
        items: array,
        _pidls: pidls,
    })
}

fn create_command(clsid: u128) -> Option<IExplorerCommand> {
    let guid = GUID::from_u128(clsid);
    unsafe { CoCreateInstance::<_, IExplorerCommand>(&guid, None, CLSCTX_ALL) }.ok()
}

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
            existing
                .item_types
                .extend(handler.item_types.iter().cloned());
            existing.packaged = existing.packaged || handler.packaged;
        })
        .or_insert(handler);
}

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
        let item_types = associations
            .get(&clsid)
            .filter(|item_types| !item_types.is_empty());
        if let Some(item_types) = item_types {
            handlers.push(HandlerRegistration {
                clsid,
                packaged: true,
                item_types: item_types.clone(),
            });
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
        let mut parts = name.split_whitespace();
        let (Some(clsid_text), Some(iid_text)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Some(clsid), Some(iid)) = (parse_guid_u128(clsid_text), parse_guid_u128(iid_text))
        else {
            continue;
        };
        let implements_explorer_command = GUID::from_u128(iid) == iexplorer_command_iid;
        if implements_explorer_command {
            clsids.push(clsid);
        }
    }
    clsids
}

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
    let wide = wide_nul(package_full_name);
    let mut length = 0u32;
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

    shell_key
        .subkeys()
        .into_iter()
        .filter_map(|verb| shell_key.open_sub(&verb))
        .filter_map(|verb_key| verb_key.read_sz("ExplorerCommandHandler"))
        .filter_map(|value| parse_guid_u128(&value))
        .collect()
}

fn per_query_extension_handlers(request: &LoadNativeMenuRequest) -> Vec<HandlerRegistration> {
    if matches!(request.target_kind, NativeMenuTargetKind::Background) {
        return Vec::new();
    }

    let mut extensions: HashSet<String> = HashSet::new();
    for path in selected_paths(request) {
        if let Some(extension) = Path::new(&path)
            .extension()
            .and_then(|value| value.to_str())
        {
            extensions.insert(format!(".{}", extension.to_lowercase()));
        }
    }

    let mut handlers = Vec::new();
    for extension in extensions {
        let classes = std::iter::once(extension.clone()).chain(class_default_value(&extension));
        for class in classes {
            let class_handlers = explorer_command_handlers_for_class(&class)
                .into_iter()
                .map(|clsid| unpackaged_handler(clsid, &extension));
            handlers.extend(class_handlers);
        }
    }
    handlers
}

fn unpackaged_handler(clsid: u128, extension: &str) -> HandlerRegistration {
    HandlerRegistration {
        clsid,
        packaged: false,
        item_types: HashSet::from([extension.to_string()]),
    }
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

struct RegKey(HKEY);

impl RegKey {
    fn open(root: HKEY, sub: &str) -> Option<Self> {
        let wide = wide_nul(sub);
        let mut handle = HKEY(null_mut());
        let result =
            unsafe { RegOpenKeyExW(root, PCWSTR(wide.as_ptr()), Some(0), KEY_READ, &mut handle) };
        if result != ERROR_SUCCESS {
            return None;
        }
        Some(Self(handle))
    }

    fn open_sub(&self, sub: &str) -> Option<Self> {
        Self::open(self.0, sub)
    }

    fn subkeys(&self) -> Vec<String> {
        enumerate_registry_names::<REGISTRY_KEY_NAME_UNITS>(|index, buffer, length| unsafe {
            RegEnumKeyExW(self.0, index, Some(buffer), length, None, None, None, None)
        })
    }

    fn value_names(&self) -> Vec<String> {
        enumerate_registry_names::<REGISTRY_VALUE_NAME_UNITS>(|index, buffer, length| unsafe {
            RegEnumValueW(self.0, index, Some(buffer), length, None, None, None, None)
        })
    }

    fn read_sz(&self, name: &str) -> Option<String> {
        let wide_name = wide_nul(name);
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
        let has_value = result == ERROR_SUCCESS && size != 0;
        if !has_value {
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

fn enumerate_registry_names<const NAME_UNITS: usize>(
    read_name_at: impl Fn(u32, PWSTR, &mut u32) -> WIN32_ERROR,
) -> Vec<String> {
    (0u32..)
        .map_while(|index| read_registry_name::<NAME_UNITS>(index, &read_name_at))
        .collect()
}

fn read_registry_name<const NAME_UNITS: usize>(
    index: u32,
    read_name_at: &impl Fn(u32, PWSTR, &mut u32) -> WIN32_ERROR,
) -> Option<String> {
    let mut buffer = [0u16; NAME_UNITS];
    let mut length = buffer.len() as u32;
    let result = read_name_at(index, PWSTR(buffer.as_mut_ptr()), &mut length);
    if result != ERROR_SUCCESS {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}
