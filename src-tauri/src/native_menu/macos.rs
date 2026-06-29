use std::process::Command;

use crate::ipc::types::MenuActionStatus;

pub fn open_with(path: &str) -> MenuActionStatus {
    if path.trim().is_empty() {
        return MenuActionStatus::unsupported("unsupported");
    }

    // We ask for the chosen app `as alias` to get a real file reference to the
    // `.app` bundle, then resolve its POSIX path and launch via the `open -a`
    // CLI. Driving Finder's `open ... using (POSIX file ...)` instead fails with
    // `-1728` ("Can't get POSIX file …"), particularly for paths with spaces, so
    // we bypass Finder entirely and let `open` handle the file path.
    let script = r#"
on run argv
  set targetPath to item 1 of argv
  set chosenApp to choose application as alias with prompt "Open With"
  set appPath to POSIX path of chosenApp
  do shell script "open -a " & quoted form of appPath & " " & quoted form of targetPath
end run
"#;

    match Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(path)
        .status()
    {
        Ok(status) if status.success() => {
            MenuActionStatus::handled_with_message("open-with-opened")
        }
        Ok(status) => {
            log::warn!("macOS Open With chooser exited with status {status}");
            MenuActionStatus::unsupported("open-with-cancelled-or-failed")
        }
        Err(error) => {
            log::warn!("macOS Open With chooser failed for {path}: {error}");
            MenuActionStatus::unsupported("open-with-launch-failed")
        }
    }
}
