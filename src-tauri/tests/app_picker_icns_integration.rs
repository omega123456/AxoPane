//! `app_picker::icns` parses hand-built byte fixtures only — no real `.icns`
//! file from disk is ever read here, keeping the test deterministic and
//! independent of any installed application.

use file_explorer_lib::app_picker::icns::{icns_to_data_url, largest_embedded_png};

fn icns_chunk(os_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend_from_slice(os_type);
    let length = (8 + payload.len()) as u32;
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(payload);
    chunk
}

fn fake_png(extra_bytes: usize) -> Vec<u8> {
    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    png.extend(std::iter::repeat_n(0xAB, extra_bytes));
    png
}

fn build_icns(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"icns".to_vec();
    let total_len = 8 + chunks.iter().map(Vec::len).sum::<usize>();
    bytes.extend_from_slice(&(total_len as u32).to_be_bytes());
    for chunk in chunks {
        bytes.extend_from_slice(chunk);
    }
    bytes
}

#[test]
fn picks_the_largest_embedded_png_among_multiple_chunks() {
    let small_png = fake_png(4);
    let large_png = fake_png(64);
    let icns = build_icns(&[
        icns_chunk(b"ic07", &small_png),
        icns_chunk(b"ic10", &large_png),
    ]);

    let found = largest_embedded_png(&icns).expect("expected a PNG payload");

    assert_eq!(found, large_png.as_slice());
}

#[test]
fn ignores_non_png_chunks() {
    let icns = build_icns(&[icns_chunk(b"it32", &[0u8; 16])]);

    assert!(largest_embedded_png(&icns).is_none());
}

#[test]
fn rejects_bytes_without_the_icns_magic() {
    let bytes = b"notanicnsfile".to_vec();

    assert!(largest_embedded_png(&bytes).is_none());
}

#[test]
fn rejects_truncated_or_out_of_bounds_chunk_lengths() {
    let mut bytes = b"icns".to_vec();
    bytes.extend_from_slice(&20u32.to_be_bytes());
    bytes.extend_from_slice(b"ic07");
    bytes.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    bytes.extend_from_slice(&[0u8; 4]);

    assert!(largest_embedded_png(&bytes).is_none());
}

#[test]
fn encodes_the_winning_png_as_a_data_url() {
    let png = fake_png(8);
    let icns = build_icns(&[icns_chunk(b"ic08", &png)]);

    let data_url = icns_to_data_url(&icns).expect("expected a data url");

    assert!(data_url.starts_with("data:image/png;base64,"));
}

#[test]
fn data_url_is_none_when_no_png_payload_exists() {
    let icns = build_icns(&[icns_chunk(b"it32", &[1, 2, 3, 4])]);

    assert!(icns_to_data_url(&icns).is_none());
}
