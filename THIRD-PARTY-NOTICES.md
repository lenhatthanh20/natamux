# Third-party notices

Natamux uses code by other people. This file lists the parts its own code is built on, with their
licenses. It is written by hand and no tool writes over it. Every other Rust crate, with its
license text, is in `THIRD-PARTY-RUST.md`, which `cargo about` makes from `Cargo.lock`.

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
  <https://github.com/lenhatthanh20/zed>. The source of the 21 crates is used without changes:
  collections, derive_refineable, gpui, gpui_apple, gpui_linux, gpui_macos, gpui_macros,
  gpui_platform, gpui_shared_string, gpui_util, gpui_web, gpui_wgpu, gpui_windows, http_client,
  path, refineable, scheduler, sum_tree, util, ztracing, ztracing_macro
  (`scripts/zed-lock-crates.txt`, checked by `scripts/zed-lock-check.py`).
  The fork changes one manifest of them: `crates/gpui_linux/Cargo.toml` takes the `xim` crates
  from <https://github.com/lenhatthanh20/xim-rs>.
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
  The UI icons in `crates/natamux-app/assets/icons/*.svg` are Lucide drawings set to the app's
  icon size, plus a few simple drawings of Natamux's own.
- material-icon-theme, MIT, Copyright 2025 Material Extensions
  (`crates/natamux-app/assets/icons/agents/LICENSE-material-icon-theme` and
  `crates/natamux-app/assets/icons/files/LICENSE-material-icon-theme`).
- lobe-icons, MIT, Copyright 2023 LobeHub
  (`crates/natamux-app/assets/icons/agents/LICENSE-lobe-icons`).

## Themes

- The Natamux themes take their colours from One Light and One Dark (MIT, Copyright 2014 GitHub
  Inc.) and other palettes. The texts are in `crates/natamux-app/assets/themes/LICENSES`.
- The JSON layout of `natamux.json`, `ayu.json` and `gruvbox.json` follows Zed's One, Ayu and
  Gruvbox theme files, which Zed ships with the MIT texts of those palettes (the same texts).

## Web bundle

The daemon embeds the web client (`packages/web/dist`). These are its npm packages: the
production dependencies of `packages/web`, as `pnpm licenses list --prod` gives them
(126 packages). `scripts/npm-licenses-check.py` fails on GPL, AGPL and SSPL, and none of them
has such a license.
The texts of the licenses follow the table.

| Package | Version | License | Copyright |
|---|---|---|---|
| @tanstack/query-core | 5.104.5 | MIT | Copyright (c) 2021-present Tanner Linsley |
| @tanstack/react-query | 5.104.5 | MIT | Copyright (c) 2021-present Tanner Linsley |
| @types/debug | 4.1.13 | MIT | Copyright (c) Microsoft Corporation. |
| @types/estree | 1.0.9 | MIT | Copyright (c) Microsoft Corporation. |
| @types/estree-jsx | 1.0.5 | MIT | Copyright (c) Microsoft Corporation. |
| @types/hast | 3.0.5 | MIT | Copyright (c) Microsoft Corporation. |
| @types/mdast | 4.0.4 | MIT | Copyright (c) Microsoft Corporation. |
| @types/ms | 2.1.0 | MIT | Copyright (c) Microsoft Corporation. |
| @types/react | 19.3.0 | MIT | Copyright (c) Microsoft Corporation. |
| @types/trusted-types | 2.0.7 | MIT | Copyright (c) Microsoft Corporation. |
| @types/unist | 2.0.11, 3.0.3 | MIT | Copyright (c) Microsoft Corporation. |
| @ungap/structured-clone | 1.4.0 | ISC | Copyright (c) 2021, Andrea Giammarchi, @WebReflection |
| @xterm/addon-fit | 0.11.0 | MIT | Copyright (c) 2019, The xterm.js authors (https://github.com/xtermjs/xterm.js) |
| @xterm/addon-search | 0.16.0 | MIT | Copyright (c) 2017, The xterm.js authors (https://github.com/xtermjs/xterm.js) |
| @xterm/addon-unicode11 | 0.9.0 | MIT | Copyright (c) 2019, The xterm.js authors (https://github.com/xtermjs/xterm.js) |
| @xterm/addon-web-links | 0.12.0 | MIT | Copyright (c) 2017, The xterm.js authors (https://github.com/xtermjs/xterm.js) |
| @xterm/addon-webgl | 0.19.0 | MIT | Copyright (c) 2018, The xterm.js authors (https://github.com/xtermjs/xterm.js) |
| @xterm/xterm | 6.0.0 | MIT | Copyright (c) 2017-2019, The xterm.js authors (https://github.com/xtermjs/xterm.js); Copyright (c) 2014-2016, SourceLair Private Company (https://www.sourcelair.com); Copyright (c) 2012-2013, Christopher Jeffrey (https://github.com/chjj/) |
| bail | 2.0.2 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| ccount | 2.0.1 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| character-entities | 2.0.2 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| character-entities-html4 | 2.1.0 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| character-entities-legacy | 3.0.0 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| character-reference-invalid | 2.0.1 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| chroma-js | 3.2.0 | (BSD-3-Clause AND Apache-2.0) | Copyright (c) 2011-2025, Gregor Aisch; Copyright (c) 2002 Cynthia Brewer, Mark Harrower, and The Pennsylvania State University |
| comma-separated-tokens | 2.0.3 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| csstype | 3.2.3 | MIT | Copyright (c) 2017-2018 Fredrik Nicol |
| debug | 4.4.3 | MIT | Copyright (c) 2014-2017 TJ Holowaychuk &lt;tj@vision-media.ca&gt;; Copyright (c) 2018-2021 Josh Junon |
| decode-named-character-reference | 1.3.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| dequal | 2.0.3 | MIT | Copyright (c) Luke Edwards &lt;luke.edwards05@gmail.com&gt; (lukeed.com) |
| devlop | 1.1.0 | MIT | Copyright (c) 2023 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| dompurify | 3.4.17 | (MPL-2.0 OR Apache-2.0) | Copyright Dr.-Ing. Mario Heiderich, Cure53 |
| escape-string-regexp | 5.0.0 | MIT | Copyright (c) Sindre Sorhus &lt;sindresorhus@gmail.com&gt; (https://sindresorhus.com) |
| estree-util-is-identifier-name | 3.0.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| extend | 3.0.2 | MIT | Copyright (c) 2014 Stefan Thomas |
| hast-util-to-jsx-runtime | 2.3.6 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| hast-util-whitespace | 3.0.0 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| html-url-attributes | 3.0.1 | MIT | Copyright (c) Titus Wormer |
| inline-style-parser | 0.2.7 | MIT | Copyright (c) 2012 TJ Holowaychuk &lt;tj@vision-media.ca&gt; |
| is-alphabetical | 2.0.1 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| is-alphanumerical | 2.0.1 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| is-decimal | 2.0.1 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| is-hexadecimal | 2.0.1 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| is-plain-obj | 4.1.0 | MIT | Copyright (c) Sindre Sorhus &lt;sindresorhus@gmail.com&gt; (https://sindresorhus.com) |
| longest-streak | 3.1.0 | MIT | Copyright (c) 2015 Titus Wormer &lt;mailto:tituswormer@gmail.com&gt; |
| lucide-react | 1.61.0 | ISC | Copyright (c) 2026 Lucide Icons and Contributors; Copyright (c) 2013-present Cole Bemis |
| markdown-table | 3.0.4 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| marked | 14.0.0 | MIT | Copyright (c) 2018+, MarkedJS (https://github.com/markedjs/); Copyright (c) 2011-2018, Christopher Jeffrey (https://github.com/chjj/); Copyright © 2004, John Gruber |
| material-icon-theme | 5.39.0 | MIT | Copyright (c) 2025 Material Extensions |
| mdast-util-find-and-replace | 3.0.3 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-from-markdown | 2.1.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-gfm | 3.1.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-gfm-autolink-literal | 2.0.1 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-gfm-footnote | 2.1.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-gfm-strikethrough | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-gfm-table | 2.0.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-gfm-task-list-item | 2.0.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-mdx-expression | 2.0.1 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-mdx-jsx | 3.2.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-mdxjs-esm | 2.0.1 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-phrasing | 4.1.0 | MIT | Copyright (c) 2017 Titus Wormer &lt;tituswormer@gmail.com&gt;; Copyright (c) 2017 Victor Felder &lt;victor@draft.li&gt; |
| mdast-util-to-hast | 13.2.1 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-to-markdown | 2.2.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| mdast-util-to-string | 4.0.0 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark | 4.0.3 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-core-commonmark | 2.0.4 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm | 3.0.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm-autolink-literal | 2.1.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm-footnote | 2.1.0 | MIT | Copyright (c) 2021 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm-strikethrough | 2.1.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm-table | 2.1.2 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm-tagfilter | 2.0.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-extension-gfm-task-list-item | 2.1.0 | MIT | Copyright (c) 2020 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-factory-destination | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-factory-label | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-factory-space | 2.1.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-factory-title | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-factory-whitespace | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-character | 2.1.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-chunked | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-classify-character | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-combine-extensions | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-decode-numeric-character-reference | 2.0.2 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-decode-string | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-edit-map | 1.0.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-encode | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-html-tag-name | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-normalize-identifier | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-resolve-all | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-sanitize-uri | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-subtokenize | 2.1.0 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-symbol | 2.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| micromark-util-types | 2.0.3 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| monaco-editor | 0.57.0 | MIT | Copyright (c) 2016 - present Microsoft Corporation |
| ms | 2.1.3 | MIT | Copyright (c) 2020 Vercel, Inc. |
| parse-entities | 4.0.2 | MIT | Copyright (c) Titus Wormer &lt;mailto:tituswormer@gmail.com&gt; |
| property-information | 7.2.0 | MIT | Copyright (c) Titus Wormer &lt;mailto:tituswormer@gmail.com&gt; |
| react | 19.3.1 | MIT | Copyright (c) Meta Platforms, Inc. and affiliates. |
| react-dom | 19.3.1 | MIT | Copyright (c) Meta Platforms, Inc. and affiliates. |
| react-markdown | 10.1.0 | MIT | Copyright (c) Espen Hovlandsdal |
| remark-gfm | 4.0.1 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| remark-parse | 11.0.0 | MIT | Copyright (c) 2014 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| remark-rehype | 11.1.2 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| remark-stringify | 11.0.0 | MIT | Copyright (c) 2014 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| scheduler | 0.28.0 | MIT | Copyright (c) Meta Platforms, Inc. and affiliates. |
| space-separated-tokens | 2.0.2 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| stringify-entities | 4.0.4 | MIT | Copyright (c) 2015 Titus Wormer &lt;mailto:tituswormer@gmail.com&gt; |
| style-to-js | 1.1.21 | MIT | Copyright (c) 2020 Menglin "Mark" Xu &lt;mark@remarkablemark.org&gt; |
| style-to-object | 1.0.14 | MIT | Copyright (c) 2017 Menglin "Mark" Xu &lt;mark@remarkablemark.org&gt; |
| trim-lines | 3.0.1 | MIT | Copyright (c) 2015 Titus Wormer &lt;mailto:tituswormer@gmail.com&gt; |
| trough | 2.2.0 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| unified | 11.0.5 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| unist-util-is | 6.0.1 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| unist-util-position | 5.0.0 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| unist-util-stringify-position | 4.0.0 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| unist-util-visit | 5.1.0 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| unist-util-visit-parents | 6.0.2 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| vfile | 6.0.3 | MIT | Copyright (c) 2015 Titus Wormer &lt;tituswormer@gmail.com&gt; |
| vfile-message | 4.0.3 | MIT | Copyright (c) Titus Wormer &lt;tituswormer@gmail.com&gt; |
| zod | 4.6.7 | MIT | Copyright (c) 2025 Colin McDonnell |
| zustand | 5.0.16 | MIT | Copyright (c) 2019 Paul Henschel |
| zwitch | 2.0.4 | MIT | Copyright (c) 2016 Titus Wormer &lt;tituswormer@gmail.com&gt; |

### Fonts of the web client

- Geist (`@fontsource-variable/geist` 5.3.0): Copyright 2024 The Geist Project Authors (https://github.com/vercel/geist-font).
- Geist Mono (`@fontsource-variable/geist-mono` 5.3.0): Copyright 2024 The Geist Project Authors (https://github.com/vercel/geist-font.git).
- Inter (`@fontsource-variable/inter` 5.3.0): Copyright 2016 The Inter Project Authors (https://github.com/rsms/inter).
- JetBrains Mono (`@fontsource-variable/jetbrains-mono` 5.3.0): Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono).

These fonts are licensed under the SIL Open Font License, Version 1.1:

```
This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

### MIT

Each package marked MIT above is under this text, with its copyright line from the table:

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### ISC

Each package marked ISC above is under this text, with its copyright line from the table.
Parts of lucide-react come from Feather (MIT, Copyright (c) 2013-present Cole Bemis).

```
Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### chroma-js (BSD-3-Clause AND Apache-2.0)

```
chroma.js - JavaScript library for color conversions

Copyright (c) 2011-2025, Gregor Aisch
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. The name Gregor Aisch may not be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL GREGOR AISCH OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT,
INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY
OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

-------------------------------------------------------

chroma.js includes colors from colorbrewer2.org, which are released under
the following license:

Copyright (c) 2002 Cynthia Brewer, Mark Harrower,
and The Pennsylvania State University.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at
http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing,
software distributed under the License is distributed on an
"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
either express or implied. See the License for the specific
language governing permissions and limitations under the License.

------------------------------------------------------

Named colors are taken from X11 Color Names.
http://www.w3.org/TR/css3-color/#svg-color

@preserve
```

### dompurify (MPL-2.0 OR Apache-2.0)

Used under Apache-2.0. The Apache-2.0 text is in `THIRD-PARTY-RUST.md`.

### monaco-editor

The package ships `ThirdPartyNotices.txt` for the code it bundles; see
<https://github.com/microsoft/monaco-editor/blob/main/ThirdPartyNotices.txt>.

### Symbols Nerd Font

The Symbols Nerd Font in `packages/web/src/assets/fonts/` has its text in
`SymbolsNerdFont-LICENSE`.
