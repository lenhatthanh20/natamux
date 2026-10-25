//! `describe('who sets the size of a shell')`.

use natamux_daemon::terminals::service::AttachOptions;

use crate::support::{Harness, Size, client, setup};

const LARGE: Size = Size {
    cols: 120,
    rows: 40,
};
const SMALL: Size = Size { cols: 80, rows: 24 };
const MIDDLE: Size = Size {
    cols: 100,
    rows: 30,
};

fn pair(size: Size) -> (u16, u16) {
    (size.cols, size.rows)
}

fn hidden() -> AttachOptions {
    AttachOptions {
        visible: Some(false),
        ..AttachOptions::default()
    }
}

fn visible_at(h: &Harness, client: &natamux_daemon::terminals::types::ClientRef, size: Size) {
    h.attach_to("T-0001", client, size, "1", AttachOptions::default());
}

#[test]
fn does_not_take_the_size_of_a_client_that_attaches_off_screen() {
    let h = setup();
    let (_, first) = client();
    let detach = h.attach_to("T-0001", &first, LARGE, "1", AttachOptions::default());
    let (hidden_client, hidden_ref) = client();

    h.attach_to("T-0001", &hidden_ref, SMALL, "1", hidden());
    assert!(h.pty(0).sizes().is_empty());
    // It never claimed, so it is the only client left and the shell keeps its size.
    detach.detach();
    assert!(h.pty(0).sizes().is_empty());

    h.settled("T-0001", "1");
    assert!(hidden_client.controls().contains(
        &natamux_proto::terminals::messages::ServerMessage::Snapshot {
            cols: LARGE.cols,
            rows: LARGE.rows,
        }
    ));
}

#[test]
fn gives_the_shell_back_the_size_of_the_last_client_that_claimed_it() {
    let h = setup();
    let (_, middle) = client();
    let (_, large) = client();
    let (_, off) = client();
    visible_at(&h, &middle, MIDDLE);
    let detach = h.attach_to("T-0001", &large, LARGE, "1", AttachOptions::default());
    h.attach_to("T-0001", &off, SMALL, "1", hidden());

    detach.detach();

    assert_eq!(h.pty(0).sizes(), [pair(LARGE), pair(MIDDLE)]);
}

fn claim_off_screen(action: &str) {
    let h = setup();
    let (_, large) = client();
    let detach = h.attach_to("T-0001", &large, LARGE, "1", AttachOptions::default());
    let (_, off) = client();
    h.attach_to("T-0001", &off, SMALL, "1", hidden());

    // The page came back: no claim.
    h.service.visible("T-0001", "1", &off, true, None);
    assert!(h.pty(0).sizes().is_empty());

    match action {
        "visible" => h.service.visible("T-0001", "1", &off, true, Some(SMALL)),
        "input" => h.service.input("T-0001", "1", Some(&off), "x"),
        _ => h.service.resize("T-0001", "1", Some(&off), SMALL),
    }
    assert_eq!(h.pty(0).sizes(), [pair(SMALL)]);
    // It claimed last, so it keeps the size when the other client leaves.
    detach.detach();
    assert_eq!(h.pty(0).sizes(), [pair(SMALL)]);
}

#[test]
fn lets_a_client_that_attached_off_screen_claim_the_size_with_visible() {
    claim_off_screen("visible");
}

#[test]
fn lets_a_client_that_attached_off_screen_claim_the_size_with_input() {
    claim_off_screen("input");
}

#[test]
fn lets_a_client_that_attached_off_screen_claim_the_size_with_resize() {
    claim_off_screen("resize");
}

#[test]
fn lets_a_client_claim_at_the_size_the_shell_has_with_no_resize() {
    let h = setup();
    let (_, first) = client();
    visible_at(&h, &first, MIDDLE);
    let (_, second) = client();
    let detach = h.attach_to("T-0001", &second, LARGE, "1", AttachOptions::default());

    h.service.visible("T-0001", "1", &first, true, Some(LARGE));
    assert_eq!(h.pty(0).sizes(), [pair(LARGE)]);
    // The first client claimed last, at the large size, so the shell keeps it.
    detach.detach();
    assert_eq!(h.pty(0).sizes(), [pair(LARGE)]);
}

#[test]
fn never_changes_the_size_for_a_client_that_leaves_the_screen() {
    let h = setup();
    let (_, first) = client();
    visible_at(&h, &first, MIDDLE);
    let (_, second) = client();
    visible_at(&h, &second, LARGE);

    h.service.visible("T-0001", "1", &first, false, Some(SMALL));
    h.service.visible("T-0001", "1", &first, false, None);

    assert_eq!(h.pty(0).sizes(), [pair(LARGE)]);
}
