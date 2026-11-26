# Asset sources

Other folders of `assets/` have their own records: `fonts/`, `icons/` (with `icons/agents/SOURCES.md`), `queries/LICENSES` and `themes/LICENSES`. Their licences are also in `THIRD-PARTY-NOTICES.md` at the repository root.

## ime-words.txt

- Used by: the input method tests (`src/ime/word_check.rs`, `src/text_view/ime_tests.rs`): 100 Vietnamese words and syllables, one per line.
- Source: added to this repository by the Natamux author in the native app spike (commit `9f54aa17b`, 2026-10-22), for the input method check described in `docs/rust-phase1.md`. No outside source is recorded in the history.
- Licence: MIT, as the rest of Natamux. Plain words are not copyrightable on their own.

## fake/

- Used by: the screenshot states and the fake daemon (`src/app/fake_git.rs`, `fake_files.rs`, `shots_text.rs`). It is not shown to users.
- `fake/code/`: sample files of about 20 languages for the code view shots. Added by the Natamux author for those states (commit `8d402d4de`, 2026-10-30). No outside source is recorded in the history.
- `fake/banner.png`: the picture of the fake `public/banner.png` in the image diff state. Added with the Changes and History screen (commit `638cbef4a`, 2026-10-23). No outside source is recorded in the history.
- Licence: MIT, as the rest of Natamux.
