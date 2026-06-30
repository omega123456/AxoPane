//! OS trash / Recycle Bin integration.
//!
//! Moving items to the OS trash mutates machine-global state, so under
//! `feature = "test-utils"` (and coverage) the real [`trash`] crate is never
//! invoked. Instead each item is relocated into a fake-trash directory under
//! the system temp dir, so tests observe the same effect (the source path is
//! gone) without ever touching the real Recycle Bin / Trash.

/// Move the given paths to the OS trash. Best-effort: the first failure aborts
/// and is reported. An empty list is a no-op.
pub fn move_to_trash(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    move_to_trash_impl(paths)
}

#[cfg(not(feature = "test-utils"))]
fn move_to_trash_impl(paths: &[String]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|error| error.to_string())
}

/// Directory the fake trash relocates items into during tests.
#[cfg(feature = "test-utils")]
pub fn fake_trash_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("axopane-fake-trash")
}

#[cfg(feature = "test-utils")]
fn ensure_fake_trash_dir(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| error.to_string())
}

#[cfg(feature = "test-utils")]
pub fn ensure_fake_trash_dir_for_tests(path: &std::path::Path) -> Result<(), String> {
    ensure_fake_trash_dir(path)
}

#[cfg(feature = "test-utils")]
#[inline(never)]
fn move_to_fake_trash_path(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    std::fs::rename(source, target).map_err(|error| error.to_string())
}

#[cfg(feature = "test-utils")]
pub fn move_to_fake_trash_path_for_tests(
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    move_to_fake_trash_path(source, target)
}

#[cfg(feature = "test-utils")]
fn move_to_trash_impl(paths: &[String]) -> Result<(), String> {
    use std::path::Path;

    let fake_trash = fake_trash_dir();
    ensure_fake_trash_dir(&fake_trash)?;

    for path in paths {
        let source = Path::new(path);
        let file_name = source
            .file_name()
            .ok_or_else(|| format!("invalid path: {path}"))?;

        // Resolve name collisions so repeated/parallel calls never fail.
        let mut target = fake_trash.join(file_name);
        let mut counter = 1;
        while target.exists() {
            target = fake_trash.join(format!("{}.{counter}", file_name.to_string_lossy()));
            counter += 1;
        }

        // The fake-trash directory is shared across parallel tests, so another
        // test may remove it between the initial setup and this rename.
        ensure_fake_trash_dir(&fake_trash)?;
        move_to_fake_trash_path(source, &target)?;
    }

    Ok(())
}
