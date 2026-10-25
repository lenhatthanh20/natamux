//! Frame borders that never fall inside a UTF-8 character (`screen-state.ts`).

/// `bytes` in frames of at most `max` bytes, none of them ending inside a UTF-8 character.
pub fn split_frames(bytes: &[u8], max: usize) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let end = char_border(bytes, start, (start + max).min(bytes.len()));
        frames.push(&bytes[start..end]);
        start = end;
    }
    frames
}

/// `end`, moved back to the start of the UTF-8 character it falls in. Never back to `start`, so a
/// piece is never empty.
pub fn char_border(bytes: &[u8], start: usize, end: usize) -> usize {
    let mut border = end;
    while border < bytes.len() && border > start && bytes[border] & 0xc0 == 0x80 {
        border -= 1;
    }
    if border > start { border } else { end }
}

/// The end of the last whole UTF-8 character. A frame, a snapshot cut or a listener's text must
/// not split one.
pub fn whole_chars(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(4) {
        let byte = bytes[bytes.len() - back];
        if byte & 0xc0 != 0x80 {
            let size = match byte {
                0xf0.. => 4,
                0xe0.. => 3,
                0xc0.. => 2,
                _ => 1,
            };
            return if size > back {
                bytes.len() - back
            } else {
                bytes.len()
            };
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_bytes_into_frames_of_at_most_the_given_size_and_no_bytes_into_no_frame() {
        assert_eq!(split_frames(b"abcdefg", 3), [&b"abc"[..], b"def", b"g"]);
        assert!(split_frames(b"", 64 * 1024).is_empty());
    }

    #[test]
    fn never_splits_an_emoji_a_vietnamese_letter_or_a_cjk_character() {
        for char in ["😀", "ệ", "中"] {
            let text = format!("a{}", char.repeat(50));
            for max in [4, 5, 6, 7] {
                let frames = split_frames(text.as_bytes(), max);
                let mut joined = String::new();
                for frame in frames {
                    assert!(!frame.is_empty() && frame.len() <= max);
                    joined.push_str(std::str::from_utf8(frame).unwrap_or("BROKEN"));
                }
                assert_eq!(joined, text);
            }
        }
    }

    #[test]
    fn finds_the_end_of_the_last_whole_character() {
        let text = "aệ".as_bytes();
        assert_eq!(whole_chars(text), 4);
        assert_eq!(whole_chars(&text[..3]), 1);
        assert_eq!(whole_chars(&text[..2]), 1);
        assert_eq!(whole_chars(b""), 0);
        assert_eq!(whole_chars(&"😀".as_bytes()[..3]), 0);
    }
}
