//! Limits of the files attached to a new task (`packages/core/src/tasks/attachments.ts`).

use serde::Serialize;

pub const MAX_ATTACHMENT_COUNT: usize = 20;
pub const MAX_ATTACHMENT_SIZE: u64 = 20_000_000;
pub const MAX_TASK_ATTACHMENT_SIZE: u64 = 50_000_000;

/// A file sent with a new task, as base64 so it fits in the JSON body (`attachmentUploadSchema`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AttachmentUpload {
    pub name: String,
    pub data: String,
}

const MB: u64 = 1_000_000;

/// Why these files (name, size) cannot all be attached to one new task, with core's words; None
/// when they can. `fitAttachments` with no files attached yet.
pub fn attachment_limit_error(files: &[(String, u64)]) -> Option<String> {
    let mut total = 0;
    for (count, (name, size)) in files.iter().enumerate() {
        if *size > MAX_ATTACHMENT_SIZE {
            return Some(format!(
                "{name} is larger than {} MB, the most for one file",
                MAX_ATTACHMENT_SIZE / MB
            ));
        }
        if count >= MAX_ATTACHMENT_COUNT {
            return Some(format!(
                "{name} is not attached: a task takes at most {MAX_ATTACHMENT_COUNT} files"
            ));
        }
        if total + size > MAX_TASK_ATTACHMENT_SIZE {
            return Some(format!(
                "{name} is not attached: the files of a task take at most {} MB together",
                MAX_TASK_ATTACHMENT_SIZE / MB
            ));
        }
        total += size;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(sizes: &[u64]) -> Vec<(String, u64)> {
        sizes
            .iter()
            .enumerate()
            .map(|(index, size)| (format!("f{index}.txt"), *size))
            .collect()
    }

    #[test]
    fn names_the_first_file_over_a_limit() {
        assert_eq!(attachment_limit_error(&files(&[10, 20])), None);
        assert_eq!(
            attachment_limit_error(&files(&[10, 20_000_001])).as_deref(),
            Some("f1.txt is larger than 20 MB, the most for one file")
        );
        assert_eq!(
            attachment_limit_error(&files(&[1; 21])).as_deref(),
            Some("f20.txt is not attached: a task takes at most 20 files")
        );
        assert_eq!(
            attachment_limit_error(&files(&[20_000_000, 20_000_000, 20_000_000])).as_deref(),
            Some("f2.txt is not attached: the files of a task take at most 50 MB together")
        );
    }
}
