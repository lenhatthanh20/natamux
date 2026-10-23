//! `describe('the size of a shell shown in several places')`.

use super::util::attach_at;
use crate::support::{Size, setup};

const SMALL: Size = Size { cols: 80, rows: 24 };
const LARGE: Size = Size {
    cols: 120,
    rows: 40,
};

fn pair(size: Size) -> (u16, u16) {
    (size.cols, size.rows)
}

#[test]
fn follows_the_client_that_typed_last() {
    let h = setup();
    let (_, left, _) = attach_at(&h, "T-0001", SMALL, "1");
    attach_at(&h, "T-0001", LARGE, "1");

    h.service.input("T-0001", "1", Some(&left), "a");
    h.service.input("T-0001", "1", Some(&left), "b");

    assert_eq!(h.pty(0).sizes(), [pair(LARGE), pair(SMALL)]);
}

#[test]
fn follows_a_client_that_sends_its_size_again_when_it_takes_the_focus() {
    let h = setup();
    let (_, left, _) = attach_at(&h, "T-0001", SMALL, "1");
    attach_at(&h, "T-0001", LARGE, "1");

    h.service.resize("T-0001", "1", Some(&left), SMALL);

    assert_eq!(h.pty(0).sizes(), [pair(LARGE), pair(SMALL)]);
}

#[test]
fn goes_back_to_the_client_used_before_when_the_last_one_leaves() {
    let h = setup();
    attach_at(&h, "T-0001", SMALL, "1");
    let (_, _, detach) = attach_at(&h, "T-0001", LARGE, "1");

    detach.detach();
    detach.detach();

    assert_eq!(h.pty(0).sizes(), [pair(LARGE), pair(SMALL)]);
}

#[test]
fn remembers_the_new_size_of_each_client() {
    let h = setup();
    let (_, left, _) = attach_at(&h, "T-0001", SMALL, "1");
    let (_, right, _) = attach_at(&h, "T-0001", LARGE, "1");
    let resized = Size {
        cols: 100,
        rows: 30,
    };

    h.service.resize("T-0001", "1", Some(&left), resized);
    h.service.input("T-0001", "1", Some(&right), "a");
    h.service.input("T-0001", "1", Some(&left), "b");

    let sizes = [LARGE, resized, LARGE, resized].map(pair);
    assert_eq!(h.pty(0).sizes(), sizes);
}
