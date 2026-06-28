use std::process::Command;

use crate::ipc::types::MenuActionStatus;

pub fn open_with(path: &str) -> MenuActionStatus {
    if path.trim().is_empty() {
        return MenuActionStatus::unsupported("unsupported");
    }

    let script = r#"
on run argv
  set targetPath to item 1 of argv
  set chosenApp to choose application with prompt "Open With"
  tell application "Finder"
    open POSIX file targetPath using chosenApp
  end tell
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
