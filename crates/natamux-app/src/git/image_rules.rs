//! Rules of the image diff (`features/git/ImageDiff.tsx`): which sides show, their captions, the
//! size text, and the pixel size read from the header of the file. Pure, no image crate.

// The image diff uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::workspaces::changes::{ChangeStatus, ImageSide};

/// The text for a 413 answer of the image route.
pub const TOO_LARGE_TEXT: &str = "This image is larger than 5 MB, so it is not shown.";

/// Decimal units, like the web client: "512 B", "12.3 KB", "4.2 MB".
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1_000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}

/// Which sides show: (before, after). A new file has no before, a deleted one has no after.
pub fn image_sides(status: ChangeStatus) -> (bool, bool) {
    match status {
        ChangeStatus::Added | ChangeStatus::Untracked => (false, true),
        ChangeStatus::Deleted => (true, false),
        _ => (true, true),
    }
}

pub fn side_caption(side: ImageSide) -> &'static str {
    match side {
        ImageSide::Original => "Before (last commit)",
        ImageSide::Modified => "After (working tree)",
    }
}

/// "Before" or "After", for the loading label and the image name.
pub fn side_short(side: ImageSide) -> &'static str {
    match side {
        ImageSide::Original => "Before",
        ImageSide::Modified => "After",
    }
}

/// The caption: the side, then the pixel size and the byte size when they are known.
pub fn caption_text(side: ImageSide, size: Option<(u32, u32)>, bytes: Option<u64>) -> String {
    let mut text = side_caption(side).to_string();
    if let Some((width, height)) = size {
        text.push_str(&format!(" · {width} × {height} px"));
    }
    if let Some(bytes) = bytes {
        text.push_str(&format!(" · {}", format_bytes(bytes)));
    }
    text
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let part = bytes.get(at..at + 3)?;
    Some(u32::from(part[0]) | (u32::from(part[1]) << 8) | (u32::from(part[2]) << 16))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return None;
    }
    // The first chunk is IHDR: width and height follow its name.
    (bytes.get(12..16)? == b"IHDR").then_some(())?;
    Some((be32(bytes, 16)?, be32(bytes, 20)?))
}

fn gif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return None;
    }
    Some((le16(bytes, 6)?, le16(bytes, 8)?))
}

fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2;
    loop {
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        // Fill bytes: any number of 0xFF may come before the marker code.
        while *bytes.get(at + 1)? == 0xFF {
            at += 1;
        }
        let marker = *bytes.get(at + 1)?;
        match marker {
            // Start and end of image, restarts and TEM have no length.
            0x01 | 0xD0..=0xD8 => at += 2,
            // The size lives in the start-of-frame markers; C4, C8 and CC are not frames.
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Some((be16(bytes, at + 7)?, be16(bytes, at + 5)?));
            }
            // End of image, or the scan began with no frame before it.
            0xD9 | 0xDA => return None,
            _ => at += 2 + be16(bytes, at + 2)? as usize,
        }
    }
}

fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    match bytes.get(12..16)? {
        b"VP8 " => {
            // After the 3-byte frame tag comes the start code, then 14 bits each.
            (bytes.get(23..26)? == [0x9D, 0x01, 0x2A]).then_some(())?;
            Some((le16(bytes, 26)? & 0x3FFF, le16(bytes, 28)? & 0x3FFF))
        }
        b"VP8L" => {
            (*bytes.get(20)? == 0x2F).then_some(())?;
            let bits = le32(bytes, 21)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => Some((le24(bytes, 24)? + 1, le24(bytes, 27)? + 1)),
        _ => None,
    }
}

/// The pixel size from the header of a PNG, GIF, JPEG or WebP; None for anything else (an SVG
/// has no pixel size) and for a header that is cut short.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let size = png_size(bytes)
        .or_else(|| gif_size(bytes))
        .or_else(|| jpeg_size(bytes))
        .or_else(|| webp_size(bytes))?;
    (size.0 > 0 && size.1 > 0).then_some(size)
}

#[cfg(test)]
#[path = "image_rules_tests.rs"]
mod tests;
