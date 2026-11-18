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
- The highlight queries come from the tree-sitter grammar repos (MIT). The Markdown queries say
  they come from nvim-treesitter (Apache-2.0); they are vendored with no changes (the `Changes:`
  line at the top of each file).
- The license texts, with their copyright lines, are in "Query file licenses" at the end of this
  file. Source: `crates/natamux-app/assets/queries/LICENSES`.

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

The Symbols Nerd Font in `packages/web/src/assets/fonts/` (source:
`SymbolsNerdFont-LICENSE` there):

```
The MIT License (MIT)

Copyright (c) 2014 Ryan L McIntyre

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

## Query file licenses

The text of `crates/natamux-app/assets/queries/LICENSES`, as the app ships the queries:

```
Licenses of the vendored query files in this folder.

The `highlights*.scm` files come from the grammar repos below, each under the MIT License.
The `extra.scm` files, `env/highlights.scm` and the `outline.scm` files are Natamux's own. The
`outline.scm` files use Natamux's own capture names (`text_view/outline.rs`) and were written
again for the MIT move in their own layout from the outline golden files (the old editor's
output) and the grammars' node names. What they share with other outline queries is what the
golden files and the grammars force: the node names of an item and the words it shows.

========================================================================
tree-sitter-bash (bash/): https://github.com/tree-sitter/tree-sitter-bash

MIT License

Copyright (c) 2017 Max Brunsfeld

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

========================================================================
tree-sitter-c (c/): https://github.com/tree-sitter/tree-sitter-c

MIT License

Copyright (c) 2014 Max Brunsfeld

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

========================================================================
tree-sitter-cpp (cpp/): https://github.com/tree-sitter/tree-sitter-cpp

MIT License

Copyright (c) 2014 Max Brunsfeld

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

========================================================================
tree-sitter-css (css/): https://github.com/tree-sitter/tree-sitter-css

MIT License

Copyright (c) 2018 Max Brunsfeld

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

========================================================================
tree-sitter-diff (diff/): https://github.com/the-mikedavis/tree-sitter-diff

MIT License

Copyright (c) 2021 Michael Davis

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

========================================================================
tree-sitter-git-commit (gitcommit/): https://github.com/the-mikedavis/tree-sitter-git-commit

MIT License

Copyright (c) 2021 Michael Davis

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

========================================================================
tree-sitter-go (go/): https://github.com/tree-sitter/tree-sitter-go

MIT License

Copyright (c) 2014 Max Brunsfeld

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

========================================================================
tree-sitter-go-mod (gomod/): https://github.com/camdencheek/tree-sitter-go-mod

MIT License

Copyright (c) 2021 Camden Cheek

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

========================================================================
tree-sitter-go-work (gowork/): https://github.com/omertuc/tree-sitter-go-work

MIT License

Copyright (c) 2021 Omer Tuchfeld
Copyright (c) 2021 Camden Cheek

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

========================================================================
tree-sitter-javascript (javascript/): https://github.com/tree-sitter/tree-sitter-javascript

MIT License

Copyright (c) 2014 Max Brunsfeld

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

========================================================================
tree-sitter-jsdoc (jsdoc/): https://github.com/tree-sitter/tree-sitter-jsdoc

MIT License

Copyright (c) 2018 Max Brunsfeld <maxbrunsfeld@gmail.com>

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

========================================================================
tree-sitter-json (json/): https://github.com/tree-sitter/tree-sitter-json

MIT License

Copyright (c) 2014 Max Brunsfeld

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

========================================================================
tree-sitter-markdown (markdown/, markdown-inline/): https://github.com/tree-sitter-grammars/tree-sitter-markdown

MIT License

Copyright (c) 2021 Matthias Deiml

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

========================================================================
tree-sitter-python (python/): https://github.com/tree-sitter/tree-sitter-python

MIT License

Copyright (c) 2016 Max Brunsfeld

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

========================================================================
tree-sitter-regex (regex/): https://github.com/tree-sitter/tree-sitter-regex

MIT License

Copyright (c) 2014 Max Brunsfeld

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

========================================================================
tree-sitter-rust (rust/): https://github.com/tree-sitter/tree-sitter-rust

MIT License

Copyright (c) 2017 Maxim Sokolov

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

========================================================================
tree-sitter-typescript (typescript/): https://github.com/tree-sitter/tree-sitter-typescript

MIT License

Copyright (c) 2017 Max Brunsfeld

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

========================================================================
tree-sitter-yaml (yaml/): https://github.com/tree-sitter-grammars/tree-sitter-yaml

MIT License

Copyright (c) 2024 tree-sitter-grammars contributors
Copyright (c) 2019-2021 Ika

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

========================================================================
nvim-treesitter: https://github.com/nvim-treesitter/nvim-treesitter

The markdown and markdown-inline highlight files say they come from nvim-treesitter,
which is under the Apache License 2.0:

                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```
