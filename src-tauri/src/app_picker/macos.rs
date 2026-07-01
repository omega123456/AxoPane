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

/// Enumerates candidate apps for the "Set Default Application…" picker.
///
/// The actual bundle scan + `Info.plist` parsing + icon resolution is
/// synchronous and can take multiple seconds (shelling out to `plutil` per
/// bundle, decoding `.icns` icons), so it is dispatched onto a dedicated
/// `tauri::async_runtime::spawn_blocking` thread and `.await`ed here rather
/// than run inline. The calling `list_applications` Tauri command is itself a
/// genuine `async fn`, so Tauri's macro-generated `body_async` already
/// dispatches it off the IPC-dispatch/main thread from the start; this
/// additional `spawn_blocking` hop ensures the (already off-main) async task
/// itself never blocks its executor thread for the whole scan duration -
/// mirroring the `set_default_application` NSWorkspace write's dispatch
/// pattern in this module.
pub async fn list_applications() -> ListApplicationsResponse {
    let task = tauri::async_runtime::spawn_blocking(|| {
        let bundles = scan::scan_app_roots(&application_roots());

        let mut apps: Vec<MacApp> = bundles
            .into_iter()
            .map(|bundle_path| {
                let plist_json =
                    read_info_plist_json(&bundle_path).unwrap_or(serde_json::json!({}));
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
    });

    task.await
        .unwrap_or_else(|_| ListApplicationsResponse { apps: Vec::new() })
}

/// Permanently associates the file's extension/type with the chosen app via
/// `NSWorkspace` (macOS 12+; see `bundle.macOS.minimumSystemVersion` in
/// `tauri.conf.json`). The write always attempts the association, even for
/// dynamic (unregistered) UTIs, and only uses the dynamic flag to choose the
/// honest failure code if/when the write is rejected - see
/// `nsworkspace::set_default_application` for the blocking-pool dispatch
/// mechanics and thread-safety notes.
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

    match nsworkspace::set_default_application(bundle_dir, extension).await {
        Ok(()) => {
            log::debug!(
                "set_default_application: NSWorkspace setDefaultApplication succeeded extension=.{extension} dynamic={dynamic} bundle_id={bundle_id}"
            );
            MenuActionStatus::handled_with_message("default-application-set")
        }
        Err(nsworkspace::WriteError::Timeout) => {
            log::debug!(
                "set_default_application: NSWorkspace setDefaultApplication timed out waiting for completion handler extension=.{extension} dynamic={dynamic} bundle_id={bundle_id}"
            );
            MenuActionStatus::unsupported("default-application-write-failed")
        }
        Err(nsworkspace::WriteError::Failed(reason)) => {
            log::debug!(
                "set_default_application: NSWorkspace setDefaultApplication failed extension=.{extension} dynamic={dynamic} bundle_id={bundle_id} reason={reason}"
            );
            if dynamic {
                MenuActionStatus::unsupported("default-application-rejected-dynamic-type")
            } else {
                MenuActionStatus::unsupported("default-application-write-failed")
            }
        }
    }
}

/// Looks up the app currently registered as the default handler for the
/// file's extension, for display in the Properties dialog. Returns `None`
/// whenever any step is inconclusive (no extension, no UTI, or no
/// registered handler resolvable to a path) rather than erroring, since "no
/// default set" is a normal, expected outcome.
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

/// Minimal `NSWorkspace`/`UTType` FFI surface, replacing the deprecated
/// LaunchServices `LSSetDefaultRoleHandlerForContentType` /
/// `LSCopyDefaultRoleHandlerForContentType` calls with the supported,
/// non-deprecated `NSWorkspace` content-type APIs (macOS 12+) via the
/// actively-maintained `objc2` crate family.
///
/// `NSWorkspace`'s default-application write
/// (`setDefaultApplicationAtURL:toOpenContentType:completionHandler:`) is
/// asynchronous; it is bridged to a blocking wait here via an Objective-C
/// completion block (`block2::RcBlock`) and a `std::sync::mpsc` channel. The
/// calling `set_default_application` Tauri command is itself a genuinely
/// `async fn`, so Tauri's macro-generated `body_async` dispatches it via
/// `crate::async_runtime::spawn` off the IPC-dispatch/main thread from the
/// start; `nsworkspace::set_default_application` further hands the blocking
/// `recv_timeout` wait to `tauri::async_runtime::spawn_blocking`'s dedicated
/// blocking-pool thread, so the calling/main thread is never blocked at any
/// point in this call chain, no matter how long the underlying macOS
/// confirmation dialog stays open. The bounded
/// [`nsworkspace::COMPLETION_TIMEOUT`] is now purely a safety net against a
/// hypothetical stuck/never-invoked completion handler - not a worst-case UI
/// freeze bound, since there is no UI freeze to bound in the first place.
mod nsworkspace {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSError, NSString, NSURL};
    use objc2_uniform_type_identifiers::UTType;

    /// Upper bound on how long we wait for `NSWorkspace` to invoke the
    /// default-application-write completion handler before giving up. Guards
    /// against a hung/never-invoked completion block (e.g. an XPC failure)
    /// permanently wedging the caller, which would otherwise leave the
    /// frontend's "Change All..." action stuck in a busy state forever.
    const COMPLETION_TIMEOUT: Duration = Duration::from_secs(10);

    /// Resolves a filename extension (without the leading dot) to a
    /// `UTType`, mirroring `UTTypeCreatePreferredIdentifierForTag` under the
    /// deprecated API. Returns `None` when the extension resolves to no
    /// known type.
    pub(super) fn uti_for_extension(extension: &str) -> Option<Retained<UTType>> {
        let tag = NSString::from_str(extension);
        UTType::typeWithFilenameExtension(&tag)
    }

    /// Whether macOS considers the resolved type "dynamic" - i.e. not a
    /// formally declared/registered UTI (common for extensions such as
    /// `.sql`, `.log`, `.env`). Used only to select the honest *failure*
    /// copy/code; the write is always attempted regardless, so a dynamic
    /// type that NSWorkspace happens to accept still reports success.
    pub(super) fn is_dynamic(uti: &UTType) -> bool {
        uti.isDynamic()
    }

    /// Best-effort string form of a `UTType`'s identifier, for diagnostics
    /// only. Never used to drive branching logic (see the module doc-comment
    /// on the historical `CFString` display artifact this sidesteps).
    pub(super) fn identifier(uti: &UTType) -> String {
        uti.identifier().to_string()
    }

    /// Builds a `file://` URL for a filesystem path (an app bundle here),
    /// suitable for the `NSWorkspace` calls below.
    pub(super) fn file_url(path: &Path) -> Retained<NSURL> {
        let ns_path = NSString::from_str(&path.to_string_lossy());
        NSURL::fileURLWithPath(&ns_path)
    }

    /// Failure outcome of [`set_default_application`], distinguishing a
    /// bounded-wait timeout (no `NSError` ever observed) from an actual
    /// `NSWorkspace`-reported failure, so the caller can report a
    /// timed-out write honestly instead of conflating it with the
    /// dynamic-type-rejection heuristic.
    pub(super) enum WriteError {
        /// The completion handler was never observed within
        /// [`COMPLETION_TIMEOUT`]; the write's outcome is unknown.
        Timeout,
        /// `NSWorkspace` invoked the completion handler with a non-null
        /// `NSError`; the string is its best-effort localized description.
        Failed(String),
    }

    /// Issues the `NSWorkspace` default-application write on a dedicated
    /// blocking-pool thread (via `tauri::async_runtime::spawn_blocking`),
    /// which blocks until the completion handler reports the outcome or
    /// [`COMPLETION_TIMEOUT`] elapses; this function `.await`s that blocking
    /// task rather than blocking the caller directly (see "Thread safety"
    /// below).
    ///
    /// Returns `Ok(())` on success, `Err(WriteError::Failed(reason))` with a
    /// best-effort human-readable failure reason on an `NSWorkspace`-reported
    /// failure, or `Err(WriteError::Timeout)` if the completion handler is
    /// never observed within the timeout.
    ///
    /// # Thread safety
    /// `NSWorkspace` invokes its completion block asynchronously - never
    /// synchronously re-entering this call. The calling
    /// `set_default_application` Tauri command is a genuine `async fn`, so
    /// Tauri's macro-generated `body_async` already dispatches it off the
    /// IPC-dispatch/main thread via `crate::async_runtime::spawn` (see the
    /// module doc-comment above). This function additionally hands the
    /// actual `NSWorkspace` call and its bounded `recv_timeout` wait to
    /// `tauri::async_runtime::spawn_blocking`, which runs the closure on a
    /// dedicated blocking-pool thread and is then `.await`ed here.
    ///
    /// Because the whole call chain is asynchronous end-to-end, the calling
    /// thread - including the app's main thread - is never blocked, no
    /// matter how long `recv_timeout` ends up waiting (e.g. while a macOS
    /// system confirmation dialog is on screen). [`COMPLETION_TIMEOUT`] is
    /// now purely a safety net against a hypothetical stuck/never-invoked
    /// completion handler, not a bound on a user-visible freeze.
    pub(super) async fn set_default_application(
        bundle_dir: &Path,
        extension: &str,
    ) -> Result<(), WriteError> {
        let bundle_dir = bundle_dir.to_path_buf();
        let extension = extension.to_string();

        let task = tauri::async_runtime::spawn_blocking(move || -> Result<(), WriteError> {
            let Some(uti) = uti_for_extension(&extension) else {
                // Already validated by the caller before spawning this
                // blocking task; this should be unreachable in practice, but
                // a failure here is treated as a normal write failure rather
                // than panicking the blocking-pool thread.
                return Err(WriteError::Failed(format!(
                    "no UTI resolved for extension .{extension} on blocking-pool thread"
                )));
            };
            let app_url = file_url(&bundle_dir);

            let (tx, rx) = mpsc::channel::<Option<String>>();

            let completion = RcBlock::new(move |error: *mut NSError| {
                let failure = if error.is_null() {
                    None
                } else {
                    // SAFETY: `NSWorkspace` passes a valid, live `NSError*`
                    // (or null) for the duration of this block invocation;
                    // it is only read here, and an owned `String` is copied
                    // out before the block returns.
                    let error = unsafe { &*error };
                    Some(error.localizedDescription().to_string())
                };
                let _ = tx.send(failure);
            });

            NSWorkspace::sharedWorkspace()
                .setDefaultApplicationAtURL_toOpenContentType_completionHandler(
                    &app_url,
                    &uti,
                    Some(&completion),
                );

            match rx.recv_timeout(COMPLETION_TIMEOUT) {
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
        });

        task.await.unwrap_or_else(|_| {
            Err(WriteError::Failed(
                "NSWorkspace blocking-pool task panicked".to_string(),
            ))
        })
    }

    /// Looks up the app currently registered to open the given content type,
    /// resolved back to a filesystem path. Content-type based end-to-end,
    /// symmetric with the write above; mirrors
    /// `LSCopyDefaultRoleHandlerForContentType` plus bundle-id-to-path
    /// resolution under the deprecated API, but without the intermediate
    /// bundle-identifier hop.
    pub(super) fn default_application_path_for_content_type(uti: &UTType) -> Option<PathBuf> {
        let app_url = NSWorkspace::sharedWorkspace().URLForApplicationToOpenContentType(uti)?;
        let path = app_url.path()?;
        Some(PathBuf::from(path.to_string()))
    }
}
