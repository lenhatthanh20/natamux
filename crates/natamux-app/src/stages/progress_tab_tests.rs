#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// The source of the function `name` in `file`, so the order of the flow's parts can be checked
/// without a window.
fn source_of(file: &'static str, name: &str) -> &'static str {
    let start = file.find(name).unwrap_or_else(|| panic!("{name}"));
    let rest = &file[start..];
    let end = rest.find("\n    }\n").expect("end of the function");
    &rest[..end]
}

/// `parts` come in this order in `source`.
fn in_order(source: &str, parts: &[&str]) {
    let at: Vec<usize> = parts
        .iter()
        .map(|part| source.find(part).unwrap_or_else(|| panic!("{part}")))
        .collect();
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{parts:?}");
}

// Mockup A (T-0257): the card sits right under the selected node's row with a caret, no
// connector between them, and the line goes on from the card's bottom. Owner, after the first
// build drew a connector and a port between the node and the card.

#[test]
fn a_stage_puts_its_card_right_under_the_node_with_no_port_between() {
    let step = source_of(include_str!("progress_flow.rs"), "fn flow_step(");
    in_order(
        step,
        &[
            "out.filter(|_| !open)",
            ".children(details)",
            "link_after(style, open)",
        ],
    );
}

#[test]
fn a_pair_puts_its_card_between_the_nodes_and_the_join_curves() {
    let pair = source_of(include_str!("progress_steps.rs"), "fn pair(");
    assert!(pair.contains("(!open).then_some"));
    assert!(pair.contains("CaretAt::Left") && pair.contains("CaretAt::Right"));
    in_order(
        pair,
        &[".child(right_node)", ".children(details)", ".child(join)"],
    );
}

#[test]
fn a_stack_and_a_branch_put_their_card_under_the_whole_frame() {
    let stack = source_of(include_str!("progress_steps.rs"), "fn stack(");
    in_order(
        stack,
        &[
            ".child(frame)",
            ".children(details)",
            "link_after(out, open)",
        ],
    );
    let branch = source_of(include_str!("progress_branch.rs"), "pub(super) fn branch(");
    in_order(
        branch,
        &[
            ".child(frame)",
            ".children(details)",
            "link_after(style, open)",
        ],
    );
}

#[test]
fn a_node_of_a_path_leaves_its_card_to_the_branch() {
    let step = source_of(include_str!("progress_flow.rs"), "fn flow_step(");
    assert!(step.contains("(!in_path)"));
    let lane = source_of(include_str!("progress_branch.rs"), "fn lane(");
    assert!(lane.contains("self.column(&steps, true, cx)"));
}
