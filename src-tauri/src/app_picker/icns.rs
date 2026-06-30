use base64::Engine;

const ICNS_MAGIC: &[u8; 4] = b"icns";
const FILE_HEADER_LEN: usize = 8;
const CHUNK_HEADER_LEN: usize = 8;
const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Walks an `.icns` file's chunk table (4-byte OSType + 4-byte big-endian
/// length + payload) and returns the largest payload that is itself a raw
/// embedded PNG (modern large-icon chunks like `ic07`-`ic14` embed PNG bytes
/// directly). Older `.icns` files using legacy RLE-only chunks have no PNG
/// payload and yield `None`, which callers treat as "no icon available".
pub fn largest_embedded_png(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < FILE_HEADER_LEN || &bytes[0..4] != ICNS_MAGIC {
        return None;
    }

    let mut offset = FILE_HEADER_LEN;
    let mut largest: Option<&[u8]> = None;

    while offset + CHUNK_HEADER_LEN <= bytes.len() {
        let length = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        if length < CHUNK_HEADER_LEN {
            break;
        }

        let payload_start = offset + CHUNK_HEADER_LEN;
        let payload_end = offset.checked_add(length)?;
        if payload_end > bytes.len() || payload_start > payload_end {
            break;
        }

        let payload = &bytes[payload_start..payload_end];
        if payload.starts_with(PNG_MAGIC) {
            let is_larger = largest.is_none_or(|current| payload.len() > current.len());
            if is_larger {
                largest = Some(payload);
            }
        }

        offset = payload_end;
    }

    largest
}

/// Encodes the largest embedded PNG as a `data:image/png;base64,...` URL,
/// matching the contract `file_icons.rs` and the frontend's `EntryIcon`
/// already use for `iconDataUrl`.
pub fn icns_to_data_url(bytes: &[u8]) -> Option<String> {
    let png = largest_embedded_png(bytes)?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}
