use crate::ipc::types::{CompressArchiveRequest, ExtractArchiveRequest, MenuActionStatus};

const UNSUPPORTED_MESSAGE: &str = "unsupported";

pub fn compress_archive(_payload: CompressArchiveRequest) -> MenuActionStatus {
    MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE)
}

pub fn extract_archive(_payload: ExtractArchiveRequest) -> MenuActionStatus {
    MenuActionStatus::unsupported(UNSUPPORTED_MESSAGE)
}
