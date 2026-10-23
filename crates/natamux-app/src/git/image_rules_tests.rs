use super::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes
}

fn gif(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes
}

/// A comment segment first, so the reader must skip a segment to find the frame.
fn jpeg(width: u16, height: u16, marker: u8) -> Vec<u8> {
    let mut bytes = vec![
        0xFF, 0xD8, 0xFF, 0xFE, 0x00, 0x04, b'h', b'i', 0xFF, marker, 0, 17, 8,
    ];
    bytes.extend(height.to_be_bytes());
    bytes.extend(width.to_be_bytes());
    bytes
}

fn webp_head(kind: &[u8; 4]) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend([0, 0, 0, 0]);
    bytes.extend(b"WEBP");
    bytes.extend(kind);
    bytes.extend([0, 0, 0, 0]);
    bytes
}

fn webp_lossy(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = webp_head(b"VP8 ");
    bytes.extend([0, 0, 0, 0x9D, 0x01, 0x2A]);
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes
}

fn webp_lossless(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = webp_head(b"VP8L");
    bytes.push(0x2F);
    bytes.extend(((width - 1) | ((height - 1) << 14)).to_le_bytes());
    bytes
}

fn webp_extended(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = webp_head(b"VP8X");
    bytes.extend([0, 0, 0, 0]);
    bytes.extend(&(width - 1).to_le_bytes()[..3]);
    bytes.extend(&(height - 1).to_le_bytes()[..3]);
    bytes
}

#[test]
fn gives_bytes_kilobytes_and_megabytes() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(12_300), "12.3 KB");
    assert_eq!(format_bytes(4_200_000), "4.2 MB");
    assert_eq!(format_bytes(999), "999 B");
    assert_eq!(format_bytes(1_000), "1.0 KB");
}

#[test]
fn reads_the_size_of_a_png() {
    assert_eq!(image_size(&png(640, 480)), Some((640, 480)));
}

#[test]
fn reads_the_size_of_a_gif() {
    assert_eq!(image_size(&gif(32, 16)), Some((32, 16)));
}

#[test]
fn reads_the_size_of_a_jpeg_past_other_segments() {
    assert_eq!(image_size(&jpeg(300, 200, 0xC0)), Some((300, 200)));
    assert_eq!(image_size(&jpeg(300, 200, 0xC2)), Some((300, 200)));
}

#[test]
fn skips_the_jpeg_markers_that_are_not_frames() {
    assert_eq!(image_size(&jpeg(300, 200, 0xC4)), None);
}

#[test]
fn reads_the_size_of_each_kind_of_webp() {
    assert_eq!(image_size(&webp_lossy(100, 50)), Some((100, 50)));
    assert_eq!(image_size(&webp_lossless(1000, 700)), Some((1000, 700)));
    assert_eq!(image_size(&webp_extended(4000, 3000)), Some((4000, 3000)));
}

#[test]
fn has_no_size_for_garbage_a_cut_header_or_an_svg() {
    assert_eq!(image_size(b""), None);
    assert_eq!(image_size(b"hello world, not an image at all"), None);
    assert_eq!(image_size(br#"<svg width="10" height="10"/>"#), None);
    assert_eq!(image_size(&png(1, 1)[..20]), None);
    assert_eq!(image_size(&gif(1, 1)[..8]), None);
    assert_eq!(image_size(&jpeg(1, 1, 0xC0)[..12]), None);
    assert_eq!(image_size(&webp_lossy(1, 1)[..27]), None);
    assert_eq!(image_size(&png(0, 5)), None);
}

#[test]
fn shows_after_only_for_new_files_and_before_only_for_deleted_files() {
    use ChangeStatus::{Added, Conflict, Deleted, Modified, Renamed, Untracked};
    assert_eq!(image_sides(Added), (false, true));
    assert_eq!(image_sides(Untracked), (false, true));
    assert_eq!(image_sides(Deleted), (true, false));
    for status in [Modified, Renamed, Conflict] {
        assert_eq!(image_sides(status), (true, true));
    }
}

#[test]
fn captions_a_side_with_its_size_and_bytes() {
    assert_eq!(
        caption_text(ImageSide::Original, None, None),
        "Before (last commit)"
    );
    assert_eq!(
        caption_text(ImageSide::Modified, Some((64, 32)), Some(12_300)),
        "After (working tree) · 64 × 32 px · 12.3 KB"
    );
    assert_eq!(
        caption_text(ImageSide::Modified, None, Some(3)),
        "After (working tree) · 3 B"
    );
    assert_eq!(side_short(ImageSide::Original), "Before");
    assert_eq!(side_short(ImageSide::Modified), "After");
}

#[test]
fn says_why_an_image_is_not_shown() {
    assert_eq!(
        TOO_LARGE_TEXT,
        "This image is larger than 5 MB, so it is not shown."
    );
}
