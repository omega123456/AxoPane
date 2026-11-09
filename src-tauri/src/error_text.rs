use std::fmt::Display;

pub trait StringifyError<T> {
    fn stringify_err(self) -> Result<T, String>;
}

impl<T, E: Display> StringifyError<T> for Result<T, E> {
    fn stringify_err(self) -> Result<T, String> {
        self.map_err(|error| error.to_string())
    }
}
