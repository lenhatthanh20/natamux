# Third-party notices

Natamux uses code by other people. This file lists the parts its own code is built on, with their
licenses. It is written by hand and no tool writes over it; a generated list of every Rust crate
comes in its own file (docs/plan-mit-license.md, section 10).

## alacritty_terminal

- Used by: the terminal (`crates/natamux-app/src/terminals/term/`), for the terminal model, its
  colour and mode types. The key and mouse bytes follow the xterm forms that Alacritty also sends.
- Source: <https://github.com/alacritty/alacritty>, through the fork
  <https://github.com/lenhatthanh20/alacritty> at commit 4c129667ce56611becdc82de6e28218c80e2e88f.
- License: Apache-2.0 (<https://www.apache.org/licenses/LICENSE-2.0>).
- Copyright 2020 The Alacritty Project.
