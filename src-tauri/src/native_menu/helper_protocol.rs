//! Bounded stdio protocol used by the Windows native-menu helper.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

use super::provider::{ProviderInvocation, ProviderNativeMenuItem};
use super::types::LoadNativeMenuRequest;
use crate::ipc::types::{MenuActionStatus, OpenWithRequest, ShowPropertiesRequest};

/// A deliberately small upper bound: native menu data is metadata, never a file payload.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", content = "payload", rename_all = "camelCase")]
pub enum HelperOperation {
    Discover(LoadNativeMenuRequest),
    Invoke(ProviderInvocation),
    Properties(ShowPropertiesRequest),
    OpenWith(OpenWithRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HelperRequest {
    pub request_id: u64,
    pub operation: HelperOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result", content = "payload", rename_all = "camelCase")]
pub enum HelperResult {
    Items(Vec<ProviderNativeMenuItem>),
    Status(MenuActionStatus),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HelperResponse {
    pub request_id: u64,
    pub result: HelperResult,
}

pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native-menu helper frame too large",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> io::Result<T> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native-menu helper frame too large",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}
