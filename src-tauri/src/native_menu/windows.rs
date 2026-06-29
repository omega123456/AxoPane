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
        let invocation = invocation.clone();
        executor.execute(move || invoke_impl(invocation))
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
    use base64::Engine;
    use png::{BitDepth, ColorType, Encoder};
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;

    use super::*;
    use crate::native_menu::types::{
        NativeMenuCanonicalActionKind, NativeMenuIcon, NativeMenuIconKind,
    };
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{
        Common::ITEMIDLIST, IContextMenu, IContextMenu2, IContextMenu3, ILFree, IShellFolder,
        SHBindToObject, SHBindToParent, SHGetDesktopFolder, SHOpenWithDialog, SHParseDisplayName,
        ShellExecuteW, CMF_CANRENAME, CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_INCLUDESTATIC,
        CMF_ITEMMENU, CMF_NODEFAULT, CMF_NORMAL, CMF_SYNCCASCADEMENU, CMINVOKECOMMANDINFO,
        GCS_VERBW, OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OPENASINFO,
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
        match enumerate_menu(request) {
            Ok(items) => items,
            Err(error) => {
                log::warn!(
                    "native menu enumeration failed for request {}: {}",
                    request.request_id,
                    error
                );
                Vec::new()
            }
        }
    }

    pub(super) fn invoke_impl(invocation: ProviderInvocation) -> MenuActionStatus {
        let ProviderInvocation::Windows {
            request,
            command_path,
        } = invocation
        else {
            return MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE);
        };

        match invoke_command(&request, &command_path) {
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

    fn classify_canonical_action(
        normalized_verb: Option<&str>,
        label: &str,
    ) -> Option<NativeMenuCanonicalActionKind> {
        let verb = normalized_verb.unwrap_or_default();
        match verb {
            "open" => Some(NativeMenuCanonicalActionKind::Open),
            "openwith" => Some(NativeMenuCanonicalActionKind::OpenWith),
            "copy" => Some(NativeMenuCanonicalActionKind::Copy),
            "cut" => Some(NativeMenuCanonicalActionKind::Cut),
            "paste" => Some(NativeMenuCanonicalActionKind::Paste),
            "rename" => Some(NativeMenuCanonicalActionKind::Rename),
            "delete" => Some(NativeMenuCanonicalActionKind::Delete),
            "properties" => Some(NativeMenuCanonicalActionKind::Properties),
            "share" => Some(NativeMenuCanonicalActionKind::Share),
            "compress" | "windowscompresszipfile" => Some(NativeMenuCanonicalActionKind::Compress),
            "extract" => Some(NativeMenuCanonicalActionKind::Extract),
            "refresh" => Some(NativeMenuCanonicalActionKind::Refresh),
            "new" | "newfolder" => Some(NativeMenuCanonicalActionKind::NewFolder),
            "selectall" => Some(NativeMenuCanonicalActionKind::SelectAll),
            _ => {
                let normalized_label = normalize_verb(label);
                match normalized_label.as_str() {
                    value if value.starts_with("openwith") => {
                        Some(NativeMenuCanonicalActionKind::OpenWith)
                    }
                    "delete" => Some(NativeMenuCanonicalActionKind::Delete),
                    "properties" => Some(NativeMenuCanonicalActionKind::Properties),
                    "copy" => Some(NativeMenuCanonicalActionKind::Copy),
                    "cut" => Some(NativeMenuCanonicalActionKind::Cut),
                    "paste" => Some(NativeMenuCanonicalActionKind::Paste),
                    "rename" => Some(NativeMenuCanonicalActionKind::Rename),
                    "refresh" => Some(NativeMenuCanonicalActionKind::Refresh),
                    "selectall" => Some(NativeMenuCanonicalActionKind::SelectAll),
                    _ => None,
                }
            }
        }
    }

    fn normalize_verb(value: &str) -> String {
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    fn sanitize_menu_label(raw: String) -> String {
        let without_shortcut = raw.split('\t').next().unwrap_or_default();
        let mut label = String::with_capacity(without_shortcut.len());
        let mut chars = without_shortcut.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '&' {
                if matches!(chars.peek(), Some('&')) {
                    label.push('&');
                    chars.next();
                }
                continue;
            }
            label.push(character);
        }
        label.trim().to_string()
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

    fn bitmap_to_data_url(bitmap: HBITMAP) -> Option<String> {
        let bitmap_bytes = bitmap_to_png_bytes(bitmap)?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bitmap_bytes)
        ))
    }

    fn bitmap_to_png_bytes(bitmap: HBITMAP) -> Option<Vec<u8>> {
        let mut object = BITMAP::default();
        let object_size = i32::try_from(std::mem::size_of::<BITMAP>()).ok()?;
        let copied = unsafe {
            GetObjectW(
                bitmap.into(),
                object_size,
                Some((&mut object as *mut BITMAP).cast()),
            )
        };
        if copied == 0 {
            return None;
        }

        let width = u32::try_from(object.bmWidth).ok()?;
        let height = u32::try_from(object.bmHeight.abs()).ok()?;
        if width == 0 || height == 0 {
            return None;
        }

        let stride = width.checked_mul(4)?;
        let pixel_bytes_len = stride.checked_mul(height)?;
        let pixel_bytes_len = usize::try_from(pixel_bytes_len).ok()?;

        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: i32::try_from(width).ok()?,
            biHeight: -i32::try_from(height).ok()?,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: u32::try_from(pixel_bytes_len).ok()?,
            ..Default::default()
        };
        let mut info = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };
        let mut pixels = vec![0u8; pixel_bytes_len];

        let device_context = unsafe { CreateCompatibleDC(None) };
        if device_context.0.is_null() {
            return None;
        }
        let dc_guard = DeviceContextGuard(device_context);

        let scan_lines = unsafe {
            GetDIBits(
                dc_guard.0,
                bitmap,
                0,
                height,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if scan_lines == 0 {
            return None;
        }

        let rgba_pixels = bgra_to_rgba(&pixels)?;
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba_pixels).ok()?;
        drop(writer);
        Some(bytes)
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

    fn is_background_target(request: &LoadNativeMenuRequest) -> bool {
        matches!(
            request.target_kind,
            super::super::types::NativeMenuTargetKind::Background
        )
    }

    fn selected_target_paths(request: &LoadNativeMenuRequest) -> Vec<String> {
        if !request.selected_paths.is_empty() {
            return request.selected_paths.clone();
        }

        request.target_path.iter().cloned().collect()
    }

    fn parent_directory(path: &str) -> Option<PathBuf> {
        let path = Path::new(path);
        path.parent().map(Path::to_path_buf)
    }

    struct OwnedPidl(*mut ITEMIDLIST);

    impl OwnedPidl {
        fn from_path(path: &str) -> windows::core::Result<Self> {
            let wide = path_to_wide(path);
            let mut pidl = null_mut();
            unsafe {
                SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)?;
            }
            Ok(Self(pidl))
        }

        fn as_ptr(&self) -> *const ITEMIDLIST {
            self.0
        }
    }

    impl Drop for OwnedPidl {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    ILFree(Some(self.0.cast_const()));
                }
            }
        }
    }

    struct MenuGuard(windows::Win32::UI::WindowsAndMessaging::HMENU);

    impl Drop for MenuGuard {
        fn drop(&mut self) {
            let _ = unsafe { DestroyMenu(self.0) };
        }
    }

    struct DeviceContextGuard(windows::Win32::Graphics::Gdi::HDC);

    impl Drop for DeviceContextGuard {
        fn drop(&mut self) {
            let _ = unsafe { DeleteDC(self.0) };
        }
    }

    fn path_to_wide(path: &str) -> Vec<u16> {
        path.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn bgra_to_rgba(pixels: &[u8]) -> Option<Vec<u8>> {
        if pixels.len() % 4 != 0 {
            return None;
        }

        let mut rgba = Vec::with_capacity(pixels.len());
        for chunk in pixels.chunks_exact(4) {
            let blue = u32::from(chunk[0]);
            let green = u32::from(chunk[1]);
            let red = u32::from(chunk[2]);
            let alpha = u32::from(chunk[3]);

            if alpha == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }

            let unpremultiply =
                |channel: u32| -> u8 { ((channel * 255 + (alpha / 2)) / alpha).min(255) as u8 };

            rgba.push(unpremultiply(red));
            rgba.push(unpremultiply(green));
            rgba.push(unpremultiply(blue));
            rgba.push(alpha as u8);
        }
        Some(rgba)
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
        let folder_path = parent_directory(first_path)?;
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
            folder_path: Some(folder_path.to_string_lossy().into_owned()),
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
