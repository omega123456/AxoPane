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

    if let Some(home) = crate::fs::home_dir() {
        roots.push(home.join("Applications"));
    }

    roots
}

fn read_info_plist_json(bundle_dir: &Path) -> Option<serde_json::Value> {
    let info_plist = bundle_dir.join("Contents").join("Info.plist");
    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&info_plist)
        .output()
        .ok()?;

    let succeeded = output.status.success();
    if !succeeded {
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

    apps.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });

    ListApplicationsResponse { apps }
}

pub async fn set_default_application(request: &SetDefaultApplicationRequest) -> MenuActionStatus {
    let Some(extension) = Path::new(&request.path)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        log::debug!(
            "set_default_application: no extension for path {}",
            request.path
        );
        return MenuActionStatus::unsupported("no-file-extension");
    };

    let bundle_dir = Path::new(&request.bundle_path);
    let Some(plist_json) = read_info_plist_json(bundle_dir) else {
        log::debug!(
            "set_default_application: could not read/parse Info.plist at {}",
            bundle_dir.display()
        );
        return MenuActionStatus::unsupported("app-info-plist-unreadable");
    };

    let meta = bundle::parse_bundle_metadata(&plist_json, bundle_dir);
    let Some(bundle_id) = meta.bundle_id else {
        log::debug!(
            "set_default_application: no CFBundleIdentifier in {}",
            bundle_dir.display()
        );
        return MenuActionStatus::unsupported("app-missing-bundle-identifier");
    };

    let Some(uti) = nsworkspace::uti_for_extension(extension) else {
        log::debug!("set_default_application: no UTI resolved for extension .{extension}");
        return MenuActionStatus::unsupported("no-uti-for-extension");
    };

    let dynamic = nsworkspace::is_dynamic(&uti);

    log::debug!(
        "set_default_application: extension=.{extension} uti={} dynamic={dynamic} bundle_id={bundle_id}",
        nsworkspace::identifier(&uti)
    );

    let write_context = format!("extension=.{extension} dynamic={dynamic} bundle_id={bundle_id}");
    let result = nsworkspace::set_default_application(bundle_dir, extension).await;
    default_application_write_status(result, dynamic, &write_context)
}

fn default_application_write_status(
    result: Result<(), nsworkspace::WriteError>,
    dynamic: bool,
    write_context: &str,
) -> MenuActionStatus {
    match result {
        Ok(()) => {
            log::debug!(
                "set_default_application: NSWorkspace setDefaultApplication succeeded {write_context}"
            );
            MenuActionStatus::handled_with_message("default-application-set")
        }
        Err(nsworkspace::WriteError::Timeout) => {
            log::debug!(
                "set_default_application: NSWorkspace setDefaultApplication timed out waiting for completion handler {write_context}"
            );
            MenuActionStatus::unsupported("default-application-write-failed")
        }
        Err(nsworkspace::WriteError::Failed(reason)) => {
            log::debug!(
                "set_default_application: NSWorkspace setDefaultApplication failed {write_context} reason={reason}"
            );
            MenuActionStatus::unsupported(write_rejection_reason(dynamic))
        }
    }
}

fn write_rejection_reason(dynamic: bool) -> &'static str {
    if dynamic {
        return "default-application-rejected-dynamic-type";
    }
    "default-application-write-failed"
}

pub fn get_default_application(request: &GetDefaultApplicationRequest) -> Option<MacApp> {
    let extension = Path::new(&request.path)
        .extension()
        .and_then(|extension| extension.to_str())?;

    let uti = nsworkspace::uti_for_extension(extension)?;
    let bundle_dir = nsworkspace::default_application_path_for_content_type(&uti)?;

    let plist_json = read_info_plist_json(&bundle_dir).unwrap_or(serde_json::json!({}));
    let meta = bundle::parse_bundle_metadata(&plist_json, &bundle_dir);

    Some(MacApp {
        name: meta.name,
        bundle_path: bundle_dir.to_string_lossy().into_owned(),
        bundle_id: meta.bundle_id,
        icon_data_url: read_icon_data_url(meta.icon_path.as_deref()),
    })
}

mod nsworkspace {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSError, NSString, NSURL};
    use objc2_uniform_type_identifiers::UTType;

    fn error_description(error: *mut NSError) -> Option<String> {
        if error.is_null() {
            return None;
        }
        // SAFETY: `NSWorkspace` passes a valid, live `NSError*` (or null) for
        // the duration of the completion block; it is only read here, and an
        // owned `String` is copied out before the block returns.
        let error = unsafe { &*error };
        Some(error.localizedDescription().to_string())
    }

    const COMPLETION_TIMEOUT: Duration = Duration::from_secs(10);

    pub(super) fn uti_for_extension(extension: &str) -> Option<Retained<UTType>> {
        let tag = NSString::from_str(extension);
        UTType::typeWithFilenameExtension(&tag)
    }

    pub(super) fn is_dynamic(uti: &UTType) -> bool {
        uti.isDynamic()
    }

    pub(super) fn identifier(uti: &UTType) -> String {
        uti.identifier().to_string()
    }

    pub(super) fn file_url(path: &Path) -> Retained<NSURL> {
        let ns_path = NSString::from_str(&path.to_string_lossy());
        NSURL::fileURLWithPath(&ns_path)
    }

    pub(super) enum WriteError {
        Timeout,
        Failed(String),
    }

    pub(super) async fn set_default_application(
        bundle_dir: &Path,
        extension: &str,
    ) -> Result<(), WriteError> {
        let bundle_dir = bundle_dir.to_path_buf();
        let extension = extension.to_string();

        let task = tauri::async_runtime::spawn_blocking(move || {
            set_default_application_blocking(&bundle_dir, &extension)
        });

        task.await.unwrap_or_else(|_| {
            Err(WriteError::Failed(
                "NSWorkspace blocking-pool task panicked".to_string(),
            ))
        })
    }

    fn set_default_application_blocking(
        bundle_dir: &Path,
        extension: &str,
    ) -> Result<(), WriteError> {
        let Some(uti) = uti_for_extension(extension) else {
            return Err(WriteError::Failed(format!(
                "no UTI resolved for extension .{extension} on blocking-pool thread"
            )));
        };
        let app_url = file_url(bundle_dir);

        let (sender, receiver) = mpsc::channel::<Option<String>>();

        let completion = RcBlock::new(move |error: *mut NSError| {
            let _ = sender.send(error_description(error));
        });

        NSWorkspace::sharedWorkspace()
            .setDefaultApplicationAtURL_toOpenContentType_completionHandler(
                &app_url,
                &uti,
                Some(&completion),
            );

        match receiver.recv_timeout(COMPLETION_TIMEOUT) {
            Ok(None) => Ok(()),
            Ok(Some(reason)) => Err(WriteError::Failed(reason)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                log::debug!(
                    "nsworkspace::set_default_application: timed out after {COMPLETION_TIMEOUT:?} waiting for NSWorkspace completion handler"
                );
                Err(WriteError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(WriteError::Failed(
                "completion handler channel disconnected".to_string(),
            )),
        }
    }

    pub(super) fn default_application_path_for_content_type(uti: &UTType) -> Option<PathBuf> {
        let app_url = NSWorkspace::sharedWorkspace().URLForApplicationToOpenContentType(uti)?;
        let path = app_url.path()?;
        Some(PathBuf::from(path.to_string()))
    }
}
