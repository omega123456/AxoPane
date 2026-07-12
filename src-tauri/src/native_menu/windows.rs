#[cfg(not(feature = "test-utils"))]
use super::helper_protocol::{HelperOperation, HelperResult};
use super::helper_supervisor::HelperRole;
#[cfg(not(feature = "test-utils"))]
use super::helper_supervisor::{DISCOVERY_DEADLINE, INVOCATION_DEADLINE};
use super::provider::{NativeMenuProvider, ProviderInvocation, ProviderNativeMenuItem};
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
            return match super::helper_supervisor::shared().call(
                HelperRole::Interactive,
                HelperOperation::Discover(request.clone()),
                DISCOVERY_DEADLINE,
            ) {
                Ok(HelperResult::Items(items)) => items,
                _ => Vec::new(),
            };
        }
        #[cfg(feature = "test-utils")]
        executor.execute({
            let request = request.clone();
            move || load_menu_impl(&request)
        })
    }

    fn invoke(
        &self,
        invocation: &ProviderInvocation,
        executor: &ShellExecutor,
    ) -> MenuActionStatus {
        #[cfg(not(feature = "test-utils"))]
        {
            let _ = executor;
            return match super::helper_supervisor::shared().call(
                HelperRole::Interactive,
                HelperOperation::Invoke(invocation.clone()),
                INVOCATION_DEADLINE,
            ) {
                Ok(HelperResult::Status(status)) => status,
                _ => MenuActionStatus::unsupported("native-helper-failed"),
            };
        }
        #[cfg(feature = "test-utils")]
        {
            let invocation = invocation.clone();
            executor.execute(move || invoke_impl(invocation))
        }
    }

    fn load_menu_for_role(
        &self,
        request: &LoadNativeMenuRequest,
        executor: &ShellExecutor,
        role: HelperRole,
    ) -> Vec<ProviderNativeMenuItem> {
        #[cfg(not(feature = "test-utils"))]
        {
            let _ = executor;
            return match super::helper_supervisor::shared().call(
                role,
                HelperOperation::Discover(request.clone()),
                DISCOVERY_DEADLINE,
            ) {
                Ok(HelperResult::Items(items)) => items,
                _ => Vec::new(),
            };
        }
        #[cfg(feature = "test-utils")]
        {
            let _ = role;
            self.load_menu(request, executor)
        }
    }
}

pub fn show_properties(request: &ShowPropertiesRequest) -> MenuActionStatus {
    #[cfg(not(feature = "test-utils"))]
    {
        return real::show_properties_impl(request);
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
        return real::open_with_impl(request);
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
mod real {
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;

    use super::*;
    use crate::native_menu::types::{
        NativeMenuCanonicalActionKind, NativeMenuIcon, NativeMenuIconKind,
    };
    use crate::native_menu::windows_shell::{
        bitmap_to_data_url, classify_canonical_action, is_background_target, normalize_verb,
        parent_directory, path_to_wide, sanitize_menu_label, selected_target_paths, OwnedPidl,
    };
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{
        Common::ITEMIDLIST, IContextMenu, IContextMenu2, IContextMenu3, IShellFolder,
        SHBindToObject, SHBindToParent, SHGetDesktopFolder, SHOpenWithDialog, ShellExecuteW,
        CMF_CANRENAME, CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_INCLUDESTATIC, CMF_ITEMMENU,
        CMF_NODEFAULT, CMF_NORMAL, CMF_SYNCCASCADEMENU, CMINVOKECOMMANDINFO, GCS_VERBW,
        OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OPENASINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuItemInfoW, MENUITEMINFOW,
        MFS_DISABLED, MFS_GRAYED, MFT_SEPARATOR, MIIM_BITMAP, MIIM_FTYPE, MIIM_ID, MIIM_STATE,
        MIIM_STRING, MIIM_SUBMENU, SW_SHOWNORMAL, WM_INITMENUPOPUP,
    };

    const COMMAND_ID_FIRST: u32 = 1;
    const COMMAND_ID_LAST: u32 = 0x7FFF;
    const MAX_SUBMENU_DEPTH: usize = 1;

    pub(super) fn load_menu_impl(request: &LoadNativeMenuRequest) -> Vec<ProviderNativeMenuItem> {
        let mut items = match enumerate_menu(request) {
            Ok(items) => items,
            Err(error) => {
                log::warn!(
                    "native menu enumeration failed for request {}: {}",
                    request.request_id,
                    error
                );
                Vec::new()
            }
        };

        // Append Windows 11 modern (IExplorerCommand) items; the shared dedupe in
        // `provider::dedupe_provider_items` removes any classic/app duplicates.
        items.extend(crate::native_menu::windows_modern::enumerate_modern_items(
            request,
        ));
        items
    }

    pub(super) fn invoke_impl(invocation: ProviderInvocation) -> MenuActionStatus {
        match invocation {
            ProviderInvocation::Windows {
                request,
                command_path,
            } => match invoke_command(&request, &command_path) {
                Ok(()) => MenuActionStatus::handled_with_message("invoked"),
                Err(error) => {
                    log::warn!(
                        "native menu invocation failed for request {} at path {:?}: {}",
                        request.request_id,
                        command_path,
                        error
                    );
                    MenuActionStatus::unsupported("native-invoke-failed")
                }
            },
            ProviderInvocation::WindowsModern {
                clsid,
                packaged,
                request,
                command_path,
            } => match crate::native_menu::windows_modern::invoke_modern(
                clsid,
                packaged,
                &request,
                &command_path,
            ) {
                Ok(()) => MenuActionStatus::handled_with_message("invoked"),
                Err(error) => {
                    log::warn!(
                        "modern menu invocation failed for request {} clsid {:032x} at path {:?}: {}",
                        request.request_id,
                        clsid,
                        command_path,
                        error
                    );
                    MenuActionStatus::unsupported("native-invoke-failed")
                }
            },
            ProviderInvocation::Fake { .. } => MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE),
        }
    }

    pub(super) fn show_properties_impl(request: &ShowPropertiesRequest) -> MenuActionStatus {
        if request.paths.is_empty() {
            return MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE);
        }

        if let Some(invoked) = invoke_canonical_action_for_paths(
            &request.paths,
            NativeMenuCanonicalActionKind::Properties,
        ) {
            return invoked;
        }

        let Some(path) = single_target_path(&request.paths) else {
            return MenuActionStatus::unsupported("properties-launch-failed");
        };

        match shell_execute("properties", path) {
            Ok(()) => MenuActionStatus::handled_with_message("properties-opened"),
            Err(error) => {
                log::warn!("windows properties launch failed for {}: {}", path, error);
                MenuActionStatus::unsupported("properties-launch-failed")
            }
        }
    }

    pub(super) fn open_with_impl(request: &OpenWithRequest) -> MenuActionStatus {
        if request.path.trim().is_empty() {
            return MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE);
        }

        match open_with_dialog(&request.path)
            .or_else(|dialog_error| {
                log::debug!(
                    "windows SHOpenWithDialog failed for {}; trying openas verb: {}",
                    request.path,
                    dialog_error
                );
                shell_execute("openas", &request.path)
            })
            .or_else(|verb_error| {
                log::debug!(
                    "windows openas verb failed for {}; trying rundll32 OpenAs_RunDLL: {}",
                    request.path,
                    verb_error
                );
                open_with_rundll32(&request.path)
            }) {
            Ok(()) => MenuActionStatus::handled_with_message("open-with-opened"),
            Err(error) => {
                log::warn!(
                    "windows open-with launch failed for {}: {}",
                    request.path,
                    error
                );
                MenuActionStatus::unsupported("open-with-launch-failed")
            }
        }
    }

    fn enumerate_menu(
        request: &LoadNativeMenuRequest,
    ) -> windows::core::Result<Vec<ProviderNativeMenuItem>> {
        let binding = MenuBinding::from_request(request)?;
        let context_menu = binding.context_menu()?;
        let popup_menu = unsafe { CreatePopupMenu()? };
        let _menu_guard = MenuGuard(popup_menu);

        unsafe {
            context_menu
                .QueryContextMenu(
                    popup_menu,
                    0,
                    COMMAND_ID_FIRST,
                    COMMAND_ID_LAST,
                    binding.query_flags(),
                )
                .ok()?;
        }

        enumerate_popup_items(request, &context_menu, popup_menu, 0, Vec::new())
    }

    fn invoke_command(
        request: &LoadNativeMenuRequest,
        command_path: &[u32],
    ) -> windows::core::Result<()> {
        let binding = MenuBinding::from_request(request)?;
        let context_menu = binding.context_menu()?;
        let popup_menu = unsafe { CreatePopupMenu()? };
        let _menu_guard = MenuGuard(popup_menu);

        unsafe {
            context_menu
                .QueryContextMenu(
                    popup_menu,
                    0,
                    COMMAND_ID_FIRST,
                    COMMAND_ID_LAST,
                    binding.query_flags(),
                )
                .ok()?;
        }

        let menu_id = menu_id_for_path(&context_menu, popup_menu, command_path)?;
        invoke_context_menu_command(&context_menu, menu_id)
    }

    struct MenuBinding {
        selection: MenuSelection,
    }

    enum MenuSelection {
        Background {
            folder: OwnedPidl,
        },
        Items {
            parent: IShellFolder,
            children: Vec<OwnedPidl>,
        },
    }

    impl MenuBinding {
        fn from_request(request: &LoadNativeMenuRequest) -> windows::core::Result<Self> {
            if is_background_target(request) {
                let folder_path = request
                    .folder_path
                    .as_deref()
                    .or(request.target_path.as_deref())
                    .ok_or_else(windows::core::Error::empty)?;
                return Ok(Self {
                    selection: MenuSelection::Background {
                        folder: OwnedPidl::from_path(folder_path)?,
                    },
                });
            }

            let paths = selected_target_paths(request);
            if paths.is_empty() {
                return Err(windows::core::Error::empty());
            }

            let first = OwnedPidl::from_path(&paths[0])?;
            let (parent, _) = unsafe { bind_to_parent(first.as_ptr())? };
            let expected_parent = parent_directory(&paths[0]);
            let mut children = Vec::with_capacity(paths.len());
            children.push(first);

            for path in paths.iter().skip(1) {
                if parent_directory(path) != expected_parent {
                    return Err(windows::core::Error::empty());
                }
                children.push(OwnedPidl::from_path(path)?);
            }

            Ok(Self {
                selection: MenuSelection::Items { parent, children },
            })
        }

        fn context_menu(&self) -> windows::core::Result<IContextMenu> {
            match &self.selection {
                MenuSelection::Background { folder } => unsafe {
                    let desktop = SHGetDesktopFolder()?;
                    let folder_shell: IShellFolder =
                        SHBindToObject(&desktop, folder.as_ptr(), None::<&IBindCtx>)?;
                    folder_shell.CreateViewObject::<IContextMenu>(HWND(null_mut()))
                },
                MenuSelection::Items { parent, children } => unsafe {
                    let child_ptrs = child_relative_pidls(children)?;
                    parent.GetUIObjectOf::<IContextMenu>(HWND(null_mut()), &child_ptrs, None)
                },
            }
        }

        fn query_flags(&self) -> u32 {
            match self.selection {
                MenuSelection::Background { .. } => {
                    CMF_NORMAL
                        | CMF_INCLUDESTATIC
                        | CMF_NODEFAULT
                        | CMF_SYNCCASCADEMENU
                        | CMF_EXTENDEDVERBS
                }
                MenuSelection::Items { .. } => {
                    CMF_NORMAL
                        | CMF_ITEMMENU
                        | CMF_INCLUDESTATIC
                        | CMF_CANRENAME
                        | CMF_EXPLORE
                        | CMF_NODEFAULT
                        | CMF_SYNCCASCADEMENU
                        | CMF_EXTENDEDVERBS
                }
            }
        }
    }

    fn child_relative_pidls(
        children: &[OwnedPidl],
    ) -> windows::core::Result<Vec<*const ITEMIDLIST>> {
        let mut relative = Vec::with_capacity(children.len());
        for child in children {
            let (_, child_relative) = unsafe { bind_to_parent(child.as_ptr())? };
            relative.push(child_relative.cast_const());
        }
        Ok(relative)
    }

    unsafe fn bind_to_parent(
        pidl: *const ITEMIDLIST,
    ) -> windows::core::Result<(IShellFolder, *mut ITEMIDLIST)> {
        let mut child = null_mut();
        let parent = SHBindToParent::<IShellFolder>(pidl, Some(&mut child))?;
        Ok((parent, child))
    }

    fn enumerate_popup_items(
        request: &LoadNativeMenuRequest,
        context_menu: &IContextMenu,
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        depth: usize,
        parent_path: Vec<u32>,
    ) -> windows::core::Result<Vec<ProviderNativeMenuItem>> {
        let count = unsafe { GetMenuItemCount(Some(menu)) };
        if count <= 0 {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();
        for index in 0..count {
            if let Some(item) = enumerate_menu_item(
                request,
                context_menu,
                menu,
                index as u32,
                depth,
                &parent_path,
            )? {
                items.push(item);
            }
        }
        Ok(items)
    }

    fn enumerate_menu_item(
        request: &LoadNativeMenuRequest,
        context_menu: &IContextMenu,
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        position: u32,
        depth: usize,
        parent_path: &[u32],
    ) -> windows::core::Result<Option<ProviderNativeMenuItem>> {
        let mut text_buffer = vec![0u16; 260];
        let mut info = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_BITMAP | MIIM_FTYPE | MIIM_STATE | MIIM_ID | MIIM_SUBMENU | MIIM_STRING,
            dwTypeData: windows::core::PWSTR(text_buffer.as_mut_ptr()),
            cch: (text_buffer.len() - 1) as u32,
            ..Default::default()
        };

        unsafe {
            GetMenuItemInfoW(menu, position, true, &mut info)?;
        }

        if info.fType == MFT_SEPARATOR {
            return Ok(None);
        }

        let label =
            sanitize_menu_label(String::from_utf16_lossy(&text_buffer[..info.cch as usize]));
        if label.is_empty() {
            return Ok(None);
        }

        let child_menu = info.hSubMenu;
        let has_submenu = !child_menu.0.is_null();
        let current_path = append_path(parent_path, position);
        let children = if has_submenu && depth < MAX_SUBMENU_DEPTH {
            initialize_submenu(context_menu, child_menu, position);
            enumerate_popup_items(
                request,
                context_menu,
                child_menu,
                depth + 1,
                current_path.clone(),
            )?
        } else {
            Vec::new()
        };

        let enabled = (info.fState & (MFS_DISABLED | MFS_GRAYED)).0 == 0;
        let normalized_verb = command_verb(context_menu, info.wID);
        let canonical_action_kind = classify_canonical_action(normalized_verb.as_deref(), &label);
        let danger = canonical_action_kind == Some(NativeMenuCanonicalActionKind::Delete);
        let icon = extract_menu_icon(&info);
        let id = format!("windows-item-{}-{}", path_key(&current_path), info.wID);
        let invocation = if has_submenu {
            None
        } else {
            Some(ProviderInvocation::Windows {
                request: request.clone(),
                command_path: current_path,
            })
        };

        Ok(Some(ProviderNativeMenuItem {
            id,
            label,
            enabled,
            danger,
            canonical_action_kind,
            normalized_verb,
            icon,
            invocation,
            children,
        }))
    }

    fn command_verb(context_menu: &IContextMenu, menu_id: u32) -> Option<String> {
        let command_index = menu_id.checked_sub(COMMAND_ID_FIRST)? as usize;
        let mut buffer = [0u16; 260];
        let result = unsafe {
            context_menu.GetCommandString(
                command_index,
                GCS_VERBW,
                None,
                windows::core::PSTR(buffer.as_mut_ptr().cast::<u8>()),
                buffer.len() as u32,
            )
        };

        if result.is_err() {
            return None;
        }

        let length = buffer.iter().position(|value| *value == 0).unwrap_or(0);
        if length == 0 {
            return None;
        }

        let normalized = normalize_verb(&String::from_utf16_lossy(&buffer[..length]));
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    fn extract_menu_icon(info: &MENUITEMINFOW) -> Option<NativeMenuIcon> {
        let bitmap = info.hbmpItem;
        if bitmap.0.is_null() {
            return None;
        }

        bitmap_to_data_url(bitmap).map(|data_url| NativeMenuIcon {
            kind: NativeMenuIconKind::DataUrl,
            data_url,
            alt: None,
        })
    }

    fn single_target_path(paths: &[String]) -> Option<&str> {
        let [path] = paths else {
            return None;
        };

        if path.trim().is_empty() {
            return None;
        }

        Some(path)
    }

    fn shell_execute(operation: &str, path: &str) -> windows::core::Result<()> {
        let operation = path_to_wide(operation);
        let target = path_to_wide(path);
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };

        let status = result.0 as isize;
        if status <= 32 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                format!("ShellExecuteW returned status code {status}"),
            ));
        }

        Ok(())
    }

    fn shell_execute_program(
        operation: &str,
        program: &str,
        parameters: &str,
    ) -> windows::core::Result<()> {
        let operation = path_to_wide(operation);
        let program = path_to_wide(program);
        let parameters = path_to_wide(parameters);
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(program.as_ptr()),
                PCWSTR(parameters.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };

        let status = result.0 as isize;
        if status <= 32 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                format!("ShellExecuteW returned status code {status}"),
            ));
        }

        Ok(())
    }

    fn open_with_dialog(path: &str) -> windows::core::Result<()> {
        let target = path_to_wide(path);
        let info = OPENASINFO {
            pcszFile: PCWSTR(target.as_ptr()),
            pcszClass: PCWSTR::null(),
            oaifInFlags: OAIF_EXEC | OAIF_ALLOW_REGISTRATION,
        };

        unsafe { SHOpenWithDialog(None, &info) }
    }

    fn open_with_rundll32(path: &str) -> windows::core::Result<()> {
        shell_execute_program(
            "open",
            "rundll32.exe",
            &format!("shell32.dll,OpenAs_RunDLL \"{path}\""),
        )
    }

    struct MenuGuard(windows::Win32::UI::WindowsAndMessaging::HMENU);

    impl Drop for MenuGuard {
        fn drop(&mut self) {
            let _ = unsafe { DestroyMenu(self.0) };
        }
    }

    fn invoke_canonical_action_for_paths(
        paths: &[String],
        kind: NativeMenuCanonicalActionKind,
    ) -> Option<MenuActionStatus> {
        let request = build_menu_request_for_paths(paths)?;
        let binding = MenuBinding::from_request(&request).ok()?;
        let context_menu = binding.context_menu().ok()?;
        let popup_menu = unsafe { CreatePopupMenu().ok()? };
        let _menu_guard = MenuGuard(popup_menu);

        unsafe {
            context_menu
                .QueryContextMenu(
                    popup_menu,
                    0,
                    COMMAND_ID_FIRST,
                    COMMAND_ID_LAST,
                    binding.query_flags(),
                )
                .ok()
                .ok()?;
        }

        let menu_id = find_menu_id_for_canonical_action(&context_menu, popup_menu, &kind)?;
        Some(match invoke_context_menu_command(&context_menu, menu_id) {
            Ok(()) => MenuActionStatus::handled_with_message("properties-opened"),
            Err(error) => {
                log::warn!(
                    "windows canonical action invocation failed for {:?}: {}",
                    kind,
                    error
                );
                MenuActionStatus::unsupported("properties-launch-failed")
            }
        })
    }

    fn build_menu_request_for_paths(paths: &[String]) -> Option<LoadNativeMenuRequest> {
        let first_path = paths.first()?.trim();
        if first_path.is_empty() {
            return None;
        }

        let first_path_buf = PathBuf::from(first_path);
        // Roots (drive letters like `D:\`, UNC shares like `\\server\share`) have
        // no parent per `Path::parent`. `folder_path` is only consulted by
        // `MenuBinding::from_request`'s background branch, which this
        // item-targeted request never takes, so a missing parent must not abort
        // the whole canonical-action attempt (that used to skip straight past
        // the real COM properties dialog to the less reliable ShellExecuteW
        // fallback for every drive/share root).
        let folder_path = parent_directory(first_path);
        let target_kind = if paths.len() > 1 {
            let all_dirs = paths.iter().all(|path| Path::new(path).is_dir());
            let all_files = paths.iter().all(|path| Path::new(path).is_file());
            if all_dirs || all_files {
                super::super::types::NativeMenuTargetKind::Multi
            } else {
                super::super::types::NativeMenuTargetKind::Mixed
            }
        } else if first_path_buf.is_dir() {
            super::super::types::NativeMenuTargetKind::Folder
        } else {
            super::super::types::NativeMenuTargetKind::File
        };

        Some(LoadNativeMenuRequest {
            request_id: "properties".to_string(),
            target_kind,
            target_path: Some(first_path.to_string()),
            folder_path: folder_path.map(|path| path.to_string_lossy().into_owned()),
            selected_paths: paths.to_vec(),
        })
    }

    fn find_menu_id_for_canonical_action(
        context_menu: &IContextMenu,
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        kind: &NativeMenuCanonicalActionKind,
    ) -> Option<u32> {
        let count = unsafe { GetMenuItemCount(Some(menu)) };
        if count <= 0 {
            return None;
        }

        for index in 0..count {
            let mut text_buffer = vec![0u16; 260];
            let mut info = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_ID | MIIM_STATE | MIIM_STRING | MIIM_SUBMENU,
                dwTypeData: windows::core::PWSTR(text_buffer.as_mut_ptr()),
                cch: (text_buffer.len() - 1) as u32,
                ..Default::default()
            };

            unsafe {
                GetMenuItemInfoW(menu, index as u32, true, &mut info).ok()?;
            }

            if info.fType == MFT_SEPARATOR {
                continue;
            }

            let label =
                sanitize_menu_label(String::from_utf16_lossy(&text_buffer[..info.cch as usize]));
            let normalized_verb = command_verb(context_menu, info.wID);
            if classify_canonical_action(normalized_verb.as_deref(), &label).as_ref() == Some(kind)
            {
                return Some(info.wID);
            }

            if !info.hSubMenu.0.is_null() {
                initialize_submenu(context_menu, info.hSubMenu, index as u32);
                if let Some(child_menu_id) =
                    find_menu_id_for_canonical_action(context_menu, info.hSubMenu, kind)
                {
                    return Some(child_menu_id);
                }
            }
        }

        None
    }

    fn append_path(parent_path: &[u32], position: u32) -> Vec<u32> {
        let mut path = Vec::with_capacity(parent_path.len() + 1);
        path.extend_from_slice(parent_path);
        path.push(position);
        path
    }

    fn path_key(path: &[u32]) -> String {
        path.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("-")
    }

    fn menu_id_for_path(
        context_menu: &IContextMenu,
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        command_path: &[u32],
    ) -> windows::core::Result<u32> {
        let (menu, position) = resolve_menu_position(context_menu, menu, command_path)?;
        let mut info = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_ID | MIIM_STATE | MIIM_SUBMENU,
            ..Default::default()
        };

        unsafe {
            GetMenuItemInfoW(menu, position, true, &mut info)?;
        }

        if !info.hSubMenu.0.is_null() {
            return Err(windows::core::Error::empty());
        }

        Ok(info.wID)
    }

    fn resolve_menu_position(
        context_menu: &IContextMenu,
        root_menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        command_path: &[u32],
    ) -> windows::core::Result<(windows::Win32::UI::WindowsAndMessaging::HMENU, u32)> {
        let mut menu = root_menu;
        let Some((last, parents)) = command_path.split_last() else {
            return Err(windows::core::Error::empty());
        };

        for position in parents {
            let mut info = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_SUBMENU,
                ..Default::default()
            };

            unsafe {
                GetMenuItemInfoW(menu, *position, true, &mut info)?;
            }

            if info.hSubMenu.0.is_null() {
                return Err(windows::core::Error::empty());
            }

            initialize_submenu(context_menu, info.hSubMenu, *position);
            menu = info.hSubMenu;
        }

        Ok((menu, *last))
    }

    fn initialize_submenu(
        context_menu: &IContextMenu,
        submenu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        position: u32,
    ) {
        let wparam = WPARAM(submenu.0 as usize);
        let lparam = LPARAM(position as isize);

        if let Ok(context_menu3) = context_menu.cast::<IContextMenu3>() {
            let mut result = LRESULT(0);
            if unsafe {
                context_menu3.HandleMenuMsg2(WM_INITMENUPOPUP, wparam, lparam, Some(&mut result))
            }
            .is_ok()
            {
                return;
            }
        }

        if let Ok(context_menu2) = context_menu.cast::<IContextMenu2>() {
            let _ = unsafe { context_menu2.HandleMenuMsg(WM_INITMENUPOPUP, wparam, lparam) };
        }
    }

    fn invoke_context_menu_command(
        context_menu: &IContextMenu,
        menu_id: u32,
    ) -> windows::core::Result<()> {
        let command_index = menu_id
            .checked_sub(COMMAND_ID_FIRST)
            .ok_or_else(windows::core::Error::empty)?;
        let mut invoke = CMINVOKECOMMANDINFO {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
            fMask: 0,
            hwnd: HWND(null_mut()),
            lpVerb: windows::core::PCSTR(command_index as usize as *const u8),
            lpParameters: windows::core::PCSTR::null(),
            lpDirectory: windows::core::PCSTR::null(),
            nShow: 1,
            dwHotKey: 0,
            hIcon: Default::default(),
        };

        unsafe { context_menu.InvokeCommand(&mut invoke) }
    }
}
