//! Ports `packages/core/src/tasks/__tests__/attachments.test.ts` (`attachmentLimitError` is tested
//! in `natamux-proto`).

use natamux_proto::tasks::attachments::{
    MAX_ATTACHMENT_COUNT, MAX_ATTACHMENT_SIZE, MAX_TASK_ATTACHMENT_SIZE,
};

use super::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn makes_a_name_safe() {
    for (name, safe) in [
        ("login-error.png", "login-error.png"),
        (
            "Screen Shot 2026-10-11 at 9.41.png",
            "Screen-Shot-2026-10-11-at-9.41.png",
        ),
        ("docs/spec.docx", "spec.docx"),
        ("C:\\Users\\me\\report.pdf", "report.pdf"),
        ("../../etc/passwd", "passwd"),
        ("..", "file"),
        (".", "file"),
        ("", "file"),
        (".env", "env"),
        ("-rf.txt", "rf.txt"),
        ("notes (1).md", "notes--1-.md"),
        ("tài liệu.xlsx", "t-i-li-u.xlsx"),
    ] {
        assert_eq!(safe_attachment_name(name), safe, "{name:?}");
    }
}

#[test]
fn keeps_a_long_name_to_100_characters_with_its_extension() {
    let safe = safe_attachment_name(&format!("{}.pdf", "a".repeat(300)));
    assert_eq!(safe.len(), 100);
    assert!(safe.ends_with("a.pdf"));
}

#[test]
fn gives_back_a_name_that_is_already_safe_as_it_is() {
    for name in ["a.png", "report_v2.final.pdf", "pasted-1.png"] {
        assert_eq!(safe_attachment_name(&safe_attachment_name(name)), name);
    }
}

#[test]
fn makes_each_name_safe_and_gives_a_repeated_name_a_number() {
    assert_eq!(
        attachment_names(&names(&[
            "screenshot.png",
            "screenshot.png",
            "a/screenshot.png",
            "notes",
            "notes"
        ])),
        [
            "screenshot.png",
            "screenshot-2.png",
            "screenshot-3.png",
            "notes",
            "notes-2"
        ]
    );
}

#[test]
fn counts_names_that_differ_only_in_case_as_the_same() {
    assert_eq!(
        attachment_names(&names(&["A.png", "a.png"])),
        ["A.png", "a-2.png"]
    );
}

#[test]
fn does_not_reuse_a_name_that_is_taken_by_a_later_file() {
    assert_eq!(
        attachment_names(&names(&["a.png", "a.png", "a-2.png"])),
        ["a.png", "a-2.png", "a-2-2.png"]
    );
}

fn file(name: &str, size: u64) -> (String, u64) {
    (name.to_string(), size)
}

fn refused(name: &str, reason: &str) -> Refused {
    Refused {
        name: name.into(),
        reason: reason.into(),
    }
}

#[test]
fn keeps_files_within_the_limits() {
    let added = vec![file("a.png", 10), file("b.pdf", MAX_ATTACHMENT_SIZE)];
    assert_eq!(fit_attachments(&[], &added), (added, vec![]));
}

#[test]
fn refuses_a_file_over_the_size_of_one_file_and_keeps_the_others() {
    let added = [file("big.mov", MAX_ATTACHMENT_SIZE + 1), file("a.png", 1)];
    let (kept, refusals) = fit_attachments(&[], &added);
    assert_eq!(kept, [file("a.png", 1)]);
    assert_eq!(
        refusals,
        [refused(
            "big.mov",
            "big.mov is larger than 20 MB, the most for one file"
        )]
    );
}

#[test]
fn refuses_the_files_after_the_20th() {
    let current: Vec<_> = (0..MAX_ATTACHMENT_COUNT - 1)
        .map(|n| file(&format!("{n}.txt"), 1))
        .collect();
    let (kept, refusals) = fit_attachments(&current, &[file("last.txt", 1), file("extra.txt", 1)]);
    assert_eq!(kept, [file("last.txt", 1)]);
    assert_eq!(
        refusals,
        [refused(
            "extra.txt",
            "extra.txt is not attached: a task takes at most 20 files"
        )]
    );
}

#[test]
fn refuses_a_file_that_takes_the_total_over_50_mb_and_keeps_a_smaller_one_after_it() {
    let current = [file("a.pdf", 20_000_000), file("b.pdf", 20_000_000)];
    let (kept, refusals) = fit_attachments(
        &current,
        &[file("c.pdf", 15_000_000), file("d.png", 5_000_000)],
    );
    assert_eq!(kept, [file("d.png", 5_000_000)]);
    assert_eq!(
        refusals,
        [refused(
            "c.pdf",
            "c.pdf is not attached: the files of a task take at most 50 MB together"
        )]
    );
    assert_eq!(MAX_TASK_ATTACHMENT_SIZE, 50_000_000);
}

#[test]
fn gives_a_file_name_its_preview_type() {
    for (name, kind) in [
        ("a.png", Some("image/png")),
        ("a.JPG", Some("image/jpeg")),
        ("a.jpeg", Some("image/jpeg")),
        ("a.gif", Some("image/gif")),
        ("a.webp", Some("image/webp")),
        ("a.svg", None),
        ("a.html", None),
        ("a.pdf", None),
        ("png", None),
    ] {
        assert_eq!(preview_type_of(name), kind, "{name}");
    }
}

#[test]
fn base64_length_is_the_length_of_the_base64_text_of_that_many_bytes() {
    // "a" is "YQ==", "abc" is "YWJj", "abcd" is "YWJjZA==".
    assert_eq!(base64_length(0), 0);
    assert_eq!(base64_length(1), 4);
    assert_eq!(base64_length(3), 4);
    assert_eq!(base64_length(4), 8);
}

#[test]
fn writes_a_size_in_b_kb_or_mb() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(12_300), "12.3 KB");
    assert_eq!(format_bytes(4_200_000), "4.2 MB");
}
