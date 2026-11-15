use super::*;

#[test]
fn a_hash_starts_a_comment_outside_quotes() {
    assert_eq!(before_hash("a = '#' # b", false), "a = '#' ");
    assert_eq!(before_hash("a#b", false), "a");
    assert_eq!(before_hash("a#b # c", true), "a#b ");
    assert_eq!(before_hash("# c", true), "");
}

#[test]
fn a_shell_line_opens_a_block_after_then_do_else_in_and_case_patterns() {
    for line in [
        "if [ -f a ]; then",
        "for a in b c; do",
        "else",
        "case $a in",
        "  a|b)",
        "*) # any",
    ] {
        assert!(shell_opens(line), "{line}");
    }
    for line in ["echo a", "f()", ")", "echo $(a)", "# then", "echo a # then"] {
        assert!(!shell_opens(line), "{line}");
    }
}

#[test]
fn a_python_line_opens_a_block_when_its_code_ends_with_a_colon() {
    for line in ["def f():", "    else:", "if a: # why", "d = {'a':"] {
        assert!(python_opens(line), "{line}");
    }
    for line in ["a = 1", "# note:", "s = 'a:'", "s = \"ab"] {
        assert!(!python_opens(line), "{line}");
    }
}

#[test]
fn a_yaml_key_with_no_value_or_a_block_mark_opens_a_block() {
    for line in ["a:", "- a:", "a: |", "a: >-", "a: |2", "a: # note"] {
        assert!(yaml_opens(line), "{line}");
    }
    for line in ["a: b", "a: \"ab", "url: http://x", "a: 1-", "# a:", "- b"] {
        assert!(!yaml_opens(line), "{line}");
    }
}
