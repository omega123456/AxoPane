//! Converts errors into the plain-string form IPC commands report to the UI.

use std::fmt::Display;

pub trait StringifyError<T> {
    /// Replaces the error with its `Display` text.
    fn stringify_err(self) -> Result<T, String>;
}

impl<T, E: Display> StringifyError<T> for Result<T, E> {
    fn stringify_err(self) -> Result<T, String> {
        self.map_err(|error| error.to_string())
    }
}
