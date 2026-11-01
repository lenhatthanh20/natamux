#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// The source of the function `name` in `file`, so the order of the flow's parts can be checked
/// without a window.
fn source_of(file: &'static str, name: &str) -> &'static str {
    let start = file.find(name).unwrap_or_else(|| panic!("{name}"));
    let rest = &file[start..];
    let end = rest.find("\n    }\n").expect("end of the function");
    &rest[..end]
}

#[test]
fn a_group_puts_the_details_card_between_its_nodes_and_its_join_dot() {
    // The card was drawn under the join dot with a second connector under it (T-0257 review 1).
    let group = source_of(include_str!("progress_steps.rs"), "pub(super) fn group(");
    let body = group.find(".child(body)").expect("body");
    let details = group.find(".children(details)").expect("details");
    let dot = group.find(".child(dot)").expect("dot");
    assert!(body < details && details < dot);
}

#[test]
fn the_card_under_a_step_leaves_groups_to_the_group() {
    let below = source_of(
        include_str!("progress_flow.rs"),
        "pub(super) fn details_below(",
    );
    assert!(below.contains("matches!(step, Step::Group(_))"));
}
