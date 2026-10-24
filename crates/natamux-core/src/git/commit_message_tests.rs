use super::*;

fn change(path: &str, status: &str, original: Option<&str>) -> ChangeEntry {
    ChangeEntry {
        path: path.into(),
        status: ChangeStatus::parse(status),
        original_path: original.map(str::to_string),
        added: None,
        removed: None,
    }
}

fn modified(path: &str) -> ChangeEntry {
    change(path, "M", None)
}

fn first_line(text: &str) -> &str {
    text.split('\n').next().unwrap_or("")
}

#[test]
fn keeps_a_short_line() {
    assert_eq!(cut_commit_subject("Fix the parser"), "Fix the parser");
}

#[test]
fn cuts_a_long_line_at_the_last_space_before_the_limit() {
    let line = format!("{}end", "word ".repeat(20));
    let cut = cut_commit_subject(&line);
    assert!(utf16_len(&cut) <= COMMIT_SUBJECT_MAX);
    assert!(line.starts_with(&cut));
    assert!(cut.ends_with("word"));
}

#[test]
fn cuts_a_long_line_without_spaces_at_the_limit() {
    assert_eq!(
        cut_commit_subject(&"a".repeat(100)),
        "a".repeat(COMMIT_SUBJECT_MAX)
    );
}

#[test]
fn names_the_one_changed_file() {
    let cases = [
        (modified("src/a.ts"), "Update src/a.ts"),
        (change("src/a.ts", "A", None), "Add src/a.ts"),
        (change("src/a.ts", "U", None), "Add src/a.ts"),
        (change("src/a.ts", "D", None), "Delete src/a.ts"),
        (
            change("src/b.ts", "R", Some("src/a.ts")),
            "Rename src/a.ts to src/b.ts",
        ),
    ];
    for (entry, message) in cases {
        assert_eq!(simple_commit_draft("Fix the parser", &[entry]), message);
    }
}

#[test]
fn keeps_the_path_as_it_is_and_needs_no_title() {
    for path in ["docs/read me.md", "src/tiếng việt.ts", "a/b/c.ts"] {
        assert_eq!(
            simple_commit_draft("", &[modified(path)]),
            format!("Update {path}")
        );
    }
}

#[test]
fn uses_the_task_title_and_lists_the_files_when_more_than_one_changed() {
    let draft = simple_commit_draft(
        "Fix the parser",
        &[
            modified("a.ts"),
            change("b.ts", "U", None),
            change("d.ts", "R", Some("c.ts")),
        ],
    );
    assert_eq!(draft, "Fix the parser\n\nM a.ts\nU b.ts\nR c.ts -> d.ts");
}

#[test]
fn lists_files_as_lines() {
    let cases = [
        (3, 3, None),
        (20, 20, None),
        (21, 20, Some("and 1 more files")),
        (50, 20, Some("and 30 more files")),
    ];
    for (count, listed, more) in cases {
        let changes: Vec<ChangeEntry> = (0..count).map(|n| modified(&format!("f{n}.ts"))).collect();
        let draft = simple_commit_draft("Fix login", &changes);
        let lines: Vec<&str> = draft.split('\n').collect();
        assert_eq!(lines[..2], ["Fix login", ""]);
        let expected: Vec<String> = changes[..listed]
            .iter()
            .map(|c| format!("M {}", c.path))
            .collect();
        assert_eq!(lines[2..2 + listed], expected[..]);
        let rest: Vec<&str> = more.into_iter().collect();
        assert_eq!(lines[2 + listed..], rest[..]);
    }
}

#[test]
fn cuts_a_title_over_72_characters_and_keeps_one_of_exactly_72() {
    let two = [modified("a.ts"), modified("b.ts")];
    let exact = "x".repeat(COMMIT_SUBJECT_MAX);
    assert_eq!(
        first_line(&simple_commit_draft(&exact, &two)),
        exact.as_str()
    );
    assert_eq!(
        first_line(&simple_commit_draft(&format!("{exact}y"), &two)),
        exact.as_str()
    );
}

#[test]
fn returns_a_string_for_no_changes_and_leaves_its_input_alone() {
    assert_eq!(simple_commit_draft("Fix login", &[]), "Fix login");
    let changes = [modified("a.ts"), change("b.ts", "D", None)];
    let copy = changes.clone();
    let first = simple_commit_draft("Fix login", &changes);
    assert_eq!(simple_commit_draft("Fix login", &changes), first);
    assert_eq!(changes, copy);
}

#[test]
fn keeps_the_first_line_within_the_limit() {
    let draft = simple_commit_draft(&"x ".repeat(60), &[modified("a.ts"), modified("b.ts")]);
    assert!(utf16_len(first_line(&draft)) <= COMMIT_SUBJECT_MAX);
    let long_path = simple_commit_draft("Fix", &[modified(&format!("{}a.ts", "dir/".repeat(30)))]);
    assert!(utf16_len(&long_path) <= COMMIT_SUBJECT_MAX);
}

#[test]
fn counts_the_files_when_the_task_has_no_title() {
    let draft = simple_commit_draft("  ", &[modified("a.ts"), modified("b.ts")]);
    assert_eq!(first_line(&draft), "Update 2 files");
}
