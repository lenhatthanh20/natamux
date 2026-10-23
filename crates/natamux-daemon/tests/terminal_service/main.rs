//! `terminal-service.test.ts`, ported. One module per `describe` group, with the same test names.
//! Where the Rust service differs (the screen parses at once, so a snapshot is sent inside
//! `attach`), a comment says what changed; cases left out are listed in docs/rust-phase4.md.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod basics;
mod part_a;
mod part_b;
mod part_c;
mod races;
