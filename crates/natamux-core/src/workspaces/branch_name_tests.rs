use super::*;

#[test]
fn builds_the_default_name_from_the_task_id_and_title() {
    assert_eq!(
        branch_name_for("T-0042", "Fix the bug"),
        "natamux/t-0042-fix-the-bug"
    );
}

#[test]
fn fills_a_pattern_or_falls_back_to_the_default() {
    for (pattern, name) in [
        (Some("{id}"), "ABC-12"),
        (Some("{id}-with-e2e"), "ABC-12-with-e2e"),
        (Some("feature/{id}-{slug}"), "feature/ABC-12-fix-it"),
        (Some("fixed"), "fixed"),
        (None, "natamux/abc-12-fix-it"),
    ] {
        assert_eq!(branch_name_from(pattern, "ABC-12", "Fix it"), name, "{pattern:?}");
    }
}

#[test]
fn accepts_only_names_git_takes_for_a_branch() {
    for name in ["ABC-12", "feature/x", "a.b", "ABC-1-with-e2e", "natamux/t-0001-fix"] {
        assert!(is_branch_name(name), "{name}");
    }
    let long = "a".repeat(MAX_BRANCH_LENGTH + 1);
    for name in [
        "",
        "@",
        "-x",
        "/x",
        "x/",
        "x.",
        "x.lock",
        "x.lock/y",
        "bad..name",
        "a//b",
        "a@{b",
        "has space",
        "a~b",
        "a^b",
        "a:b",
        "a?b",
        "a*b",
        "a[b",
        "a\\b",
        ".x",
        "x/.y",
        "a\tb",
        long.as_str(),
    ] {
        assert!(!is_branch_name(name), "{name:?}");
    }
}
