//! The screen of a shell, kept the way a terminal shows it (L1). Programs such as agents place
//! text with cursor moves for the size the shell had at that moment, so the raw output shows
//! garbage on a screen of another size. A dashboard that connects gets this screen instead, as a
//! snapshot: escape sequences that xterm.js replays to the same cells, cursor and modes. Ported
//! from `packages/daemon/src/terminals/screen-state.ts`, on `alacritty_terminal`.

pub mod frames;
mod screen;
mod serialize;

pub use screen::Screen;
