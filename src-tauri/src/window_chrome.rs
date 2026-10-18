//! Native window chrome: the title bar's theme and the Windows 11 Mica backdrop.

use std::sync::OnceLock;

use serde::Serialize;
use tauri::{Runtime, Theme, WebviewWindow};

/// First Windows 11 build. Mica needs it; Windows 10 keeps the solid base.
pub const MICA_MIN_BUILD: u32 = 22_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowBackdrop {
    Mica,
    Solid,
}

static BACKDROP: OnceLock<WindowBackdrop> = OnceLock::new();

pub fn backdrop_for_build(build: Option<u32>) -> WindowBackdrop {
    match build {
        Some(build) if build >= MICA_MIN_BUILD => WindowBackdrop::Mica,
        _ => WindowBackdrop::Solid,
    }
}

/// Maps the app's theme to the native title bar's; `None` follows the OS.
pub fn native_theme(theme: Option<&str>) -> Option<Theme> {
    match theme {
        Some("light") => Some(Theme::Light),
        Some("dark") => Some(Theme::Dark),
        _ => None,
    }
}

/// Applies Mica where the OS supports it and records which backdrop the window
/// got, so the frontend knows whether its base surfaces must be transparent.
pub fn apply_backdrop<R: Runtime>(window: &WebviewWindow<R>) -> WindowBackdrop {
    let backdrop = match backdrop_for_build(platform::windows_build()) {
        WindowBackdrop::Mica => platform::apply_mica(window),
        WindowBackdrop::Solid => WindowBackdrop::Solid,
    };
    let _ = BACKDROP.set(backdrop);
    backdrop
}

/// The backdrop recorded at startup; solid until one was applied.
pub fn current_backdrop() -> WindowBackdrop {
    BACKDROP.get().copied().unwrap_or(WindowBackdrop::Solid)
}

#[cfg(any(feature = "test-utils", not(windows)))]
mod platform {
    use tauri::{Runtime, WebviewWindow};

    use super::WindowBackdrop;

    /// Tests and non-Windows builds never read the OS build, so they always
    /// render the solid fallback.
    pub fn windows_build() -> Option<u32> {
        None
    }

    pub fn apply_mica<R: Runtime>(_window: &WebviewWindow<R>) -> WindowBackdrop {
        WindowBackdrop::Solid
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
mod platform {
    use tauri::window::{Effect, EffectsBuilder};
    use tauri::{Runtime, WebviewWindow};
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    use super::WindowBackdrop;

    /// `CurrentBuildNumber` is reliable without an OS-compatibility manifest,
    /// unlike `GetVersionEx`.
    pub fn windows_build() -> Option<u32> {
        let mut buffer = [0u16; 16];
        let mut size = (buffer.len() * std::mem::size_of::<u16>()) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
                w!("CurrentBuildNumber"),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size as *mut u32),
            )
        };
        if status.is_err() {
            return None;
        }
        let length = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..length])
            .trim()
            .parse()
            .ok()
    }

    pub fn apply_mica<R: Runtime>(window: &WebviewWindow<R>) -> WindowBackdrop {
        match window.set_effects(EffectsBuilder::new().effect(Effect::Mica).build()) {
            Ok(()) => WindowBackdrop::Mica,
            Err(error) => {
                log::warn!("Mica backdrop unavailable, using the solid base: {error}");
                WindowBackdrop::Solid
            }
        }
    }
}
