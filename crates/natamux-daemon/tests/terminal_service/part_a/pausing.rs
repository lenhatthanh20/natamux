//! Pausing a shell: it ends as a closed one does, but its clients hear that it paused.

use natamux_daemon::terminals::service::AttachOptions;
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::Signal;

use super::util::{attach_at, signals};
use crate::support::{SIZE, client, setup};

#[test]
fn ends_the_shell_and_tells_its_clients_after_how_many_idle_minutes_it_paused() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    let (watching, _, _) = attach_at(&h, "T-0001", SIZE, "2");

    assert!(h.service.pause_one("T-0001", "2", 30));
    h.pump();

    assert_eq!(h.service.list("T-0001"), ["1"]);
    assert_eq!(
        watching.controls().last(),
        Some(&ServerMessage::Paused { minutes: 30 })
    );
    assert_eq!(signals(&h, 1), [Signal::Hup]);
    assert!(!h.service.pause_one("T-0001", "2", 30));
}

#[test]
fn counts_a_shell_as_watched_only_while_a_client_shows_it() {
    let h = setup();
    assert!(!h.service.watched("T-0001", "1"));

    let (_, shown, _) = attach_at(&h, "T-0001", SIZE, "1");
    assert!(h.service.watched("T-0001", "1"));

    h.service.visible("T-0001", "1", &shown, false, None, false);
    assert!(!h.service.watched("T-0001", "1"));

    let (_, hidden) = client();
    let options = AttachOptions {
        visible: Some(false),
        ..AttachOptions::default()
    };
    let _detach = h.attach_to("T-0001", &hidden, SIZE, "1", options);
    assert!(!h.service.watched("T-0001", "1"));
}
