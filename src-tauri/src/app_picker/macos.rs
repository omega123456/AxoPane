use std::path::{Path, PathBuf};
use std::process::Command;

use super::types::{
    GetDefaultApplicationRequest, ListApplicationsResponse, MacApp, SetDefaultApplicationRequest,
};
use super::{bundle, icns, scan};
use crate::ipc::types::MenuActionStatus;

fn application_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    roots
}

/// Shells out to the stock `plutil` CLI to convert a bundle's `Info.plist`
/// (binary or XML) into JSON, reusing the already-present `serde_json`
/// instead of adding a plist-parsing crate. Returns `None` on any failure so
/// callers can skip the bundle gracefully rather than aborting the scan.
fn read_info_plist_json(bundle_dir: &Path) -> Option<serde_json::Value> {
    let info_plist = bundle_dir.join("Contents").join("Info.plist");
    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&info_plist)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    serde_json::from_slice(&output.stdout).ok()
}

fn read_icon_data_url(icon_path: Option<&Path>) -> Option<String> {
    let bytes = std::fs::read(icon_path?).ok()?;
    icns::icns_to_data_url(&bytes)
}

pub fn list_applications() -> ListApplicationsResponse {
    let bundles = scan::scan_app_roots(&application_roots());

    let mut apps: Vec<MacApp> = bundles
        .into_iter()
        .map(|bundle_path| {
            let plist_json = read_info_plist_json(&bundle_path).unwrap_or(serde_json::json!({}));
            let meta = bundle::parse_bundle_metadata(&plist_json, &bundle_path);
            MacApp {
                name: meta.name,
                bundle_path: bundle_path.to_string_lossy().into_owned(),
                bundle_id: meta.bundle_id,
                icon_data_url: read_icon_data_url(meta.icon_path.as_deref()),
            }
        })
        .collect();

    apps.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    ListApplicationsResponse { apps }
}

pub fn set_default_application(request: &SetDefaultApplicationRequest) -> MenuActionStatus {
    let Some(extension) = Path::new(&request.path)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return MenuActionStatus::unsupported("no-file-extension");
    };

    let bundle_dir = Path::new(&request.bundle_path);
    let Some(plist_json) = read_info_plist_json(bundle_dir) else {
        return MenuActionStatus::unsupported("app-info-plist-unreadable");
    };

    let meta = bundle::parse_bundle_metadata(&plist_json, bundle_dir);
    let Some(bundle_id) = meta.bundle_id else {
        return MenuActionStatus::unsupported("app-missing-bundle-identifier");
    };

    let Some(uti) = launch_services::uti_for_extension(extension) else {
        return MenuActionStatus::unsupported("no-uti-for-extension");
    };

    if launch_services::set_default_role_handler(&uti, &bundle_id) {
        MenuActionStatus::handled_with_message("default-application-set")
    } else {
        MenuActionStatus::unsupported("launch-services-write-failed")
    }
}

/// Looks up the app currently registered as the default handler for the
/// file's extension, for display in the Properties dialog. Returns `None`
/// whenever any step is inconclusive (no extension, no UTI, no registered
/// handler, or the handler's bundle can't be resolved/parsed) rather than
/// erroring, since "no default set" is a normal, expected outcome.
pub fn get_default_application(request: &GetDefaultApplicationRequest) -> Option<MacApp> {
    let extension = Path::new(&request.path)
        .extension()
        .and_then(|extension| extension.to_str())?;

    let uti = launch_services::uti_for_extension(extension)?;
    let bundle_id = launch_services::default_bundle_id_for_uti(&uti)?;
    let bundle_dir = launch_services::application_path_for_bundle_id(&bundle_id)?;

    let plist_json = read_info_plist_json(&bundle_dir).unwrap_or(serde_json::json!({}));
    let meta = bundle::parse_bundle_metadata(&plist_json, &bundle_dir);

    Some(MacApp {
        name: meta.name,
        bundle_path: bundle_dir.to_string_lossy().into_owned(),
        bundle_id: meta.bundle_id.or(Some(bundle_id)),
        icon_data_url: read_icon_data_url(meta.icon_path.as_deref()),
    })
}

/// Minimal LaunchServices/CoreServices FFI surface. The UTI/role-handler
/// functions are deprecated since macOS 12 but remain functional and have no
/// non-deprecated C-callable replacement usable without Swift/ObjC bridging.
/// `LSCopyApplicationURLsForBundleIdentifier` (macOS 10.13+) is the current,
/// non-deprecated way to resolve a bundle identifier back to an install
/// path. Kept deliberately tiny: a handful of `extern "C"` calls plus
/// `core-foundation`'s safe `CFString` for ref-counted string handling; the
/// `CFArray`/`CFURL` results are opaque pointers consumed through a couple of
/// hand-declared `CoreFoundation` accessors rather than pulling in those
/// `core-foundation` wrapper modules for a single call site.
mod launch_services {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;
    use std::path::PathBuf;

    const K_LS_ROLES_ALL: u32 = 0xFFFF_FFFF;
    const K_CFURL_POSIX_PATH_STYLE: i32 = 0;

    type CFArrayRef = *const c_void;
    type CFURLRef = *const c_void;

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn LSSetDefaultRoleHandlerForContentType(
            in_content_type: CFStringRef,
            in_role: u32,
            in_handler_bundle_id: CFStringRef,
        ) -> i32;

        fn LSCopyDefaultRoleHandlerForContentType(
            in_content_type: CFStringRef,
            in_role: u32,
        ) -> CFStringRef;

        fn LSCopyApplicationURLsForBundleIdentifier(
            in_bundle_identifier: CFStringRef,
            out_error: *mut c_void,
        ) -> CFArrayRef;

        fn UTTypeCreatePreferredIdentifierForTag(
            in_tag_class: CFStringRef,
            in_tag: CFStringRef,
            in_conforming_to_uti: CFStringRef,
        ) -> CFStringRef;

        static kUTTagClassFilenameExtension: CFStringRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: CFArrayRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> *const c_void;
        fn CFURLCopyFileSystemPath(url: CFURLRef, path_style: i32) -> CFStringRef;
        fn CFRelease(value: *const c_void);
    }

    pub(super) fn uti_for_extension(extension: &str) -> Option<CFString> {
        let tag = CFString::new(extension);

        let uti_ref = unsafe {
            UTTypeCreatePreferredIdentifierForTag(
                kUTTagClassFilenameExtension,
                tag.as_concrete_TypeRef(),
                std::ptr::null(),
            )
        };

        if uti_ref.is_null() {
            return None;
        }

        Some(unsafe { CFString::wrap_under_create_rule(uti_ref) })
    }

    pub(super) fn set_default_role_handler(uti: &CFString, bundle_id: &str) -> bool {
        let handler = CFString::new(bundle_id);

        let status = unsafe {
            LSSetDefaultRoleHandlerForContentType(
                uti.as_concrete_TypeRef(),
                K_LS_ROLES_ALL,
                handler.as_concrete_TypeRef(),
            )
        };

        status == 0
    }

    pub(super) fn default_bundle_id_for_uti(uti: &CFString) -> Option<String> {
        let handler_ref = unsafe {
            LSCopyDefaultRoleHandlerForContentType(uti.as_concrete_TypeRef(), K_LS_ROLES_ALL)
        };

        if handler_ref.is_null() {
            return None;
        }

        let handler = unsafe { CFString::wrap_under_create_rule(handler_ref) };
        Some(handler.to_string())
    }

    pub(super) fn application_path_for_bundle_id(bundle_id: &str) -> Option<PathBuf> {
        let bundle_id_ref = CFString::new(bundle_id);

        let array_ref = unsafe {
            LSCopyApplicationURLsForBundleIdentifier(
                bundle_id_ref.as_concrete_TypeRef(),
                std::ptr::null_mut(),
            )
        };

        if array_ref.is_null() {
            return None;
        }

        let path = first_array_url_as_path(array_ref);
        unsafe { CFRelease(array_ref) };
        path
    }

    fn first_array_url_as_path(array_ref: CFArrayRef) -> Option<PathBuf> {
        if unsafe { CFArrayGetCount(array_ref) } == 0 {
            return None;
        }

        let url_ref = unsafe { CFArrayGetValueAtIndex(array_ref, 0) } as CFURLRef;
        let path_ref = unsafe { CFURLCopyFileSystemPath(url_ref, K_CFURL_POSIX_PATH_STYLE) };
        if path_ref.is_null() {
            return None;
        }

        let path_string = unsafe { CFString::wrap_under_create_rule(path_ref) };
        Some(PathBuf::from(path_string.to_string()))
    }
}
