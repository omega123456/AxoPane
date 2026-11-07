use file_explorer_lib::pixels::{
    encode_rgba_png, png_data_url, premultiplied_bgra_to_rgba, unpremultiply,
};

#[test]
fn unpremultiply_restores_straight_alpha_and_zeroes_transparent_pixels() {
    assert_eq!(unpremultiply(64, 128), 128);
    assert_eq!(unpremultiply(255, 255), 255);
    assert_eq!(unpremultiply(10, 0), 0);
}

#[test]
fn premultiplied_bgra_becomes_rgba() {
    let bgra = [10, 20, 30, 255, 1, 2, 3, 0];
    assert_eq!(
        premultiplied_bgra_to_rgba(&bgra),
        Some(vec![30, 20, 10, 255, 0, 0, 0, 0])
    );
}

#[test]
fn partial_pixels_are_rejected() {
    assert_eq!(premultiplied_bgra_to_rgba(&[1, 2, 3]), None);
}

#[test]
fn rgba_encodes_to_a_png_data_url() {
    let png = encode_rgba_png(1, 1, &[255, 0, 0, 255]).expect("png");
    assert!(png.starts_with(b"\x89PNG"));
    assert!(png_data_url(&png).starts_with("data:image/png;base64,iVBOR"));
}
