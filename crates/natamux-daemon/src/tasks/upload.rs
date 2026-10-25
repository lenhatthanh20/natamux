//! Files sent with a new task, as base64 (`decodeAttachments`). Checked before anything is
//! written, so a file over a limit makes no task.

use natamux_core::tasks::attachments::attachment_names;
use natamux_core::tasks::inputs::Upload;
use natamux_proto::tasks::attachments::attachment_limit_error;
use natamux_store::attachments::AttachmentFile;

use crate::tasks::errors::{TaskError, TaskResult};

pub fn decode_attachments(uploads: &[Upload]) -> TaskResult<Vec<AttachmentFile>> {
    let decoded: Vec<(String, Vec<u8>)> = uploads
        .iter()
        .map(|upload| (upload.name.clone(), decode_base64(&upload.data)))
        .collect();
    let sizes: Vec<(String, u64)> = decoded
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.len() as u64))
        .collect();
    if let Some(error) = attachment_limit_error(&sizes) {
        return Err(TaskError::Attachment(error));
    }
    let names = attachment_names(
        &decoded
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
    );
    Ok(names
        .into_iter()
        .zip(decoded)
        .map(|(name, (_, bytes))| AttachmentFile { name, bytes })
        .collect())
}

/// `Buffer.from(text, 'base64')`: both alphabets, other characters skipped, `=` ends the data.
pub fn decode_base64(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut bits: u32 = 0;
    let mut count = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        bits = (bits << 6) | u32::from(value);
        count += 1;
        if count == 4 {
            out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            bits = 0;
            count = 0;
        }
    }
    match count {
        2 => out.push((bits >> 4) as u8),
        3 => out.extend_from_slice(&[(bits >> 10) as u8, (bits >> 2) as u8]),
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_base64_as_node_does() {
        assert_eq!(decode_base64("aGVsbG8="), b"hello");
        assert_eq!(decode_base64("aGVs bG8"), b"hello");
        assert_eq!(decode_base64("-_8"), [0xfb, 0xff]);
        assert_eq!(decode_base64("aGk=extra"), b"hi");
    }

    #[test]
    fn refuses_more_than_twenty_files_and_names_them_safely() {
        let upload = |name: &str| Upload {
            name: name.into(),
            data: "aGk=".into(),
        };
        let files = decode_attachments(&[upload("../a b.png"), upload("A B.png")]).unwrap();
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, ["a-b.png", "A-B-2.png"]);
        let many: Vec<Upload> = (0..21)
            .map(|index| upload(&format!("f{index}.txt")))
            .collect();
        assert!(matches!(
            decode_attachments(&many),
            Err(TaskError::Attachment(text)) if text == "f20.txt is not attached: a task takes at most 20 files"
        ));
    }

    fn encode_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let bits = chunk.iter().enumerate().fold(0u32, |bits, (at, byte)| {
                bits | u32::from(*byte) << (16 - 8 * at)
            });
            for at in 0..=chunk.len() {
                out.push(char::from(ALPHABET[(bits >> (18 - 6 * at) & 63) as usize]));
            }
            out.extend(std::iter::repeat_n('=', 3 - chunk.len()));
        }
        out
    }

    fn text_upload(name: &str, text: &str) -> Upload {
        Upload {
            name: name.into(),
            data: encode_base64(text.as_bytes()),
        }
    }

    #[test]
    fn decodes_the_files_and_makes_their_names_safe_and_unique() {
        let files = decode_attachments(&[
            text_upload("../shot.png", "one"),
            text_upload("shot.png", "two"),
            text_upload("My notes.md", "# Notes"),
        ])
        .unwrap();
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, ["shot.png", "shot-2.png", "My-notes.md"]);
        let bytes: Vec<&[u8]> = files.iter().map(|file| file.bytes.as_slice()).collect();
        assert_eq!(bytes, [&b"one"[..], b"two", b"# Notes"]);
    }

    #[test]
    fn keeps_the_bytes_of_a_binary_file_as_they_are() {
        let bytes = [0u8, 255, 137, 80, 78, 71, 13, 10, 26, 10];
        let upload = Upload {
            name: "a.png".into(),
            data: encode_base64(&bytes),
        };
        let files = decode_attachments(&[upload]).unwrap();
        assert_eq!(files[0].bytes, bytes);
    }

    #[test]
    fn refuses_files_over_a_limit_with_the_name_of_the_file() {
        // 'A' is six bits of zero: 26_666_668 of them decode to 20_000_001 bytes, one over.
        let big = Upload {
            name: "big.bin".into(),
            data: "A".repeat(26_666_668),
        };
        assert!(matches!(
            decode_attachments(&[text_upload("a.txt", "a"), big]),
            Err(TaskError::Attachment(text)) if text == "big.bin is larger than 20 MB, the most for one file"
        ));
    }

    #[test]
    fn refuses_a_21st_file() {
        let uploads: Vec<Upload> = (0..21)
            .map(|n| text_upload(&format!("f{n}.txt"), "x"))
            .collect();
        assert!(matches!(
            decode_attachments(&uploads),
            Err(TaskError::Attachment(_))
        ));
    }
}
