---
title: Licensing
description: The licence of everything Natamux ships, where the notice files are, and what Natamux claims about the code that agents write with it.
sidebar:
  order: 5
---

This page is for a legal team that checks Natamux before a company builds a product with it. [For a security review](/docs/help/security-review/) covers the technical controls.

## 1. The licence of each group that ships

| Group | Licence | Where to read it |
|---|---|---|
| Natamux (the app, the daemon, the `natamux` command) | MIT | [`LICENSE`](https://github.com/lenhatthanh20/natamux/blob/main/LICENSE) |
| The 18 Zed crates (the window and drawing layer), from the fork `github.com/lenhatthanh20/zed` | Apache-2.0 | "Zed" in [`THIRD-PARTY-NOTICES.md`](https://github.com/lenhatthanh20/natamux/blob/main/THIRD-PARTY-NOTICES.md) |
| Other crates | One of 13 allowed ids: MIT, MPL-2.0, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, BSL-1.0, bzip2-1.0.6, CC0-1.0, CDLA-Permissive-2.0, ISC, Unicode-3.0, Zlib. `deny.toml` and `about.toml` hold the same list, and a test checks it | [`THIRD-PARTY-RUST.md`](https://github.com/lenhatthanh20/natamux/blob/main/THIRD-PARTY-RUST.md) |
| Fonts: IBM Plex Sans and Lilex | SIL Open Font License 1.1 | "Fonts" in `THIRD-PARTY-NOTICES.md` |
| Icons: Lucide, material-icon-theme, lobe-icons, and a few agent logos | ISC and MIT; the aider mark is Apache-2.0 | "Icons" in `THIRD-PARTY-NOTICES.md`, and [`SOURCES.md`](https://github.com/lenhatthanh20/natamux/blob/main/crates/natamux-app/assets/icons/agents/SOURCES.md) of the agent icons |
| Highlight queries | MIT; the Markdown queries come from nvim-treesitter (Apache-2.0) | "Query files" in `THIRD-PARTY-NOTICES.md` |
| Themes (colours of known palettes) | MIT | "Themes" in `THIRD-PARTY-NOTICES.md` |
| SQLite, compiled into the daemon | Public domain (the SQLite blessing) | [sqlite.org/copyright.html](https://www.sqlite.org/copyright.html) |

The Zed fork's git history is the full record of its changes. `THIRD-PARTY-NOTICES.md` lists the changed files.

## 2. Where the notice files are

`LICENSE`, `THIRD-PARTY-NOTICES.md` and `THIRD-PARTY-RUST.md` are in every package:

| Package | Folder |
|---|---|
| macOS app | `Natamux.app/Contents/Resources/` |
| AppImage | The top folder of the image |
| `.deb` | `/opt/Natamux/` |
| `.tar.gz` | The top folder, next to the programs |
| Windows install | The install folder, next to `natamux-app.exe` |

## 3. Code that agents write with Natamux

Natamux adds no licence term, no attribution and no claim to the code, the commits or the pull requests that agents write with it. They belong to you under your own terms, and under the terms of the agent you use. The commit message that Natamux drafts has no `Co-Authored-By` line and no "Generated with" line: the draft prompt forbids them and Natamux removes them.

## 4. Earlier releases are GPL

The releases `native-beta.1` and `v1.0.0` to the last `v1.0.x` were released under GPL-3.0-or-later and stay under it. The first release after the move to MIT, and every later one, is MIT.

## 5. Trademarks

Agent names and logos (Claude, Codex, Gemini, Copilot, Cursor and the others) are trademarks of their owners. Natamux shows them only to name the agent. It has no affiliation with them and no endorsement from them.

## 6. The website

The packages of the website (`site/`) are not part of the app and do not ship in any release file.

## 7. The SBOM

Each release has one SBOM per platform, in CycloneDX 1.5 JSON: `Natamux-mac-arm64.cdx.json`, `Natamux-mac-x64.cdx.json`, `Natamux-win-x64.cdx.json` and `Natamux-linux-x64.cdx.json`. They are on the release page, in `SHA256SUMS.txt`, and have a build provenance attestation. They list the licence of each crate that the crate gives, and the bundled SQLite. See [What a release is built from](/docs/help/security-review/#what-a-release-is-built-from).

## Contributing

Contributions are MIT. See [`CONTRIBUTING.md`](https://github.com/lenhatthanh20/natamux/blob/main/CONTRIBUTING.md).
