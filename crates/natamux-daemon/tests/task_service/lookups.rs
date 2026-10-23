//! The group 'getTask and listEvents'.

use natamux_daemon::tasks::errors::TaskError;

use super::*;

#[test]
fn fails_with_not_found_for_an_unknown_task() {
    let fx = Fixture::new();
    assert!(matches!(
        fx.service.get_task("T-0099"),
        Err(TaskError::NotFound(_))
    ));
    assert!(matches!(
        fx.service.list_events("T-0099"),
        Err(TaskError::NotFound(_))
    ));
}
