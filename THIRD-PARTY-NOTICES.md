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

## Zed

- Used by: the app's window and drawing layer (`gpui`, `gpui_platform`, `util` and the crates they
  need).
- Source: <https://github.com/zed-industries/zed>, through the fork
  <https://github.com/lenhatthanh20/zed>. The crates are used without changes. The list is
  `scripts/zed-lock-crates.txt`.
- License: Apache-2.0 (<https://www.apache.org/licenses/LICENSE-2.0>).
- Copyright 2022 - 2025 Zed Industries, Inc. Zed has no NOTICE file.

## Query files

- Used by: the syntax colours and the outline of the code views
  (`crates/natamux-app/assets/queries/`).
- The highlight queries come from the tree-sitter grammar repos (MIT). The nvim-treesitter Markdown
  queries are Apache-2.0, with the changes made to them. The license texts and the list of changes
  are in `crates/natamux-app/assets/queries/LICENSES`.

## Fonts

- IBM Plex Sans, Copyright 2017 IBM Corp. SIL Open Font License 1.1
  (`crates/natamux-app/assets/fonts/ibm-plex-sans/license.txt`).
- Lilex, Copyright 2019 The Lilex Project Authors. SIL Open Font License 1.1
  (`crates/natamux-app/assets/fonts/lilex/OFL.txt`).
- The web client also ships fonts. Their notices are in the web bundle section below.

## Icons

- Lucide, ISC, Copyright 2026 Lucide Icons and Contributors; parts derived from Feather (MIT)
  (`crates/natamux-app/assets/icons/lucide/LICENSE`).
- material-icon-theme, MIT, Copyright 2025 Material Extensions
  (`crates/natamux-app/assets/icons/agents/LICENSE-material-icon-theme` and
  `crates/natamux-app/assets/icons/files/LICENSE-material-icon-theme`).
- lobe-icons, MIT, Copyright 2023 LobeHub
  (`crates/natamux-app/assets/icons/agents/LICENSE-lobe-icons`).

## Themes

- The Natamux themes take their colours from One Light and One Dark (MIT, Copyright 2014 GitHub
  Inc.) and other palettes. The texts are in `crates/natamux-app/assets/themes/LICENSES`.

## Web bundle

- The daemon embeds the web client (`packages/web/dist`). Its npm packages, with their license and
  copyright line, and the Geist, Geist Mono, Inter and JetBrains Mono fonts with the OFL text,
  are added here in the last step of the license move, from the output of `Tool: npm-licenses`.
  `scripts/npm-licenses-check.py` fails on GPL.
- The Symbols Nerd Font in `packages/web/src/assets/fonts/` has its text in
  `SymbolsNerdFont-LICENSE`.
