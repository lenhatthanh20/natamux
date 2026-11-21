//! Checks the app icons: `resources/icon.icns` (macOS), `icon.ico` (Windows exe and installer) and
//! `icon-256.png` (Linux window and packages).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Each type the file must hold once, with the width of its PNG.
const SIZES: [(&str, u32); 8] = [
    ("ic11", 32),
    ("ic12", 64),
    ("ic07", 128),
    ("ic13", 256),
    ("ic08", 256),
    ("ic14", 512),
    ("ic09", 512),
    ("ic10", 1024),
];

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes[..4].try_into().unwrap())
}

/// The type and the PNG width and height of each entry, in file order.
fn entries() -> Vec<(String, u32, u32)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/icon.icns");
    let data = std::fs::read(path).unwrap();
    assert!(
        data.len() >= 8,
        "icon.icns is {} bytes, too short for the header",
        data.len()
    );
    assert_eq!(&data[..4], b"icns");
    assert_eq!(be32(&data[4..]) as usize, data.len(), "header length");
    let mut out = Vec::new();
    let mut at = 8;
    while at < data.len() {
        assert!(at + 8 <= data.len(), "cut entry header at byte {at}");
        let kind = String::from_utf8(data[at..at + 4].to_vec()).unwrap();
        let len = be32(&data[at + 4..]) as usize;
        assert!(
            len > 8 && at + len <= data.len(),
            "{kind}: bad length {len}"
        );
        let png = &data[at + 8..at + len];
        assert!(png.len() >= 24, "{kind}: PNG too short");
        assert!(png.starts_with(PNG_MAGIC), "{kind}: not a PNG");
        out.push((kind, be32(&png[16..]), be32(&png[20..])));
        at += len;
    }
    out
}

// 1Password's access dialog draws the first entry of the file at 128 px. With the 32 px entry
// first, the icon there was blurry. 1Password's own file starts with a 256 px entry.
#[test]
fn first_entry_is_256_px() {
    let first = entries().into_iter().next().unwrap();
    assert_eq!(first, ("ic13".to_string(), 256, 256));
}

#[test]
fn has_every_size_once() {
    let entries = entries();
    assert_eq!(entries.len(), SIZES.len(), "{entries:?}");
    for (kind, width) in SIZES {
        let found: Vec<_> = entries.iter().filter(|e| e.0 == kind).collect();
        assert_eq!(found, [&(kind.to_string(), width, width)], "{kind}");
    }
}

fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes[..2].try_into().unwrap())
}

#[test]
fn windows_icon_has_small_and_large_sizes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/icon.ico");
    let data = std::fs::read(path).unwrap();
    assert!(data.len() >= 6, "icon.ico too short");
    assert_eq!(le16(&data), 0, "reserved");
    assert_eq!(le16(&data[2..]), 1, "type must be icon");
    let count = le16(&data[4..]) as usize;
    assert!(data.len() >= 6 + 16 * count, "cut directory");
    // In the directory, a width byte of 0 means 256.
    let widths: Vec<u32> = (0..count)
        .map(|i| match data[6 + 16 * i] {
            0 => 256,
            w => u32::from(w),
        })
        .collect();
    for size in [16, 24, 32, 48, 64, 128, 256] {
        assert!(widths.contains(&size), "no {size} px entry in {widths:?}");
    }
}

#[test]
fn linux_icon_is_a_256_px_png() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/icon-256.png");
    let png = std::fs::read(path).unwrap();
    assert!(png.len() >= 24 && png.starts_with(PNG_MAGIC), "not a PNG");
    assert_eq!((be32(&png[16..]), be32(&png[20..])), (256, 256));
}
