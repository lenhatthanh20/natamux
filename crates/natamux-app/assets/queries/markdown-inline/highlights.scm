; Vendored from tree-sitter-grammars/tree-sitter-markdown (fork lenhatthanh20/tree-sitter-markdown @b596e73), tree-sitter-markdown-inline/queries/highlights.scm.
; MIT License, Copyright (c) 2021 Matthias Deiml. Full text in ../LICENSES.
; The file says it comes from nvim-treesitter (Apache-2.0); that notice is in ../LICENSES too.
; Changes: none.

;; From nvim-treesitter/nvim-treesitter
[
  (code_span)
  (link_title)
] @text.literal

[
  (emphasis_delimiter)
  (code_span_delimiter)
] @punctuation.delimiter

(emphasis) @text.emphasis

(strong_emphasis) @text.strong

[
  (link_destination)
  (uri_autolink)
] @text.uri

[
  (link_label)
  (link_text)
  (image_description)
] @text.reference

[
  (backslash_escape)
  (hard_line_break)
] @string.escape

(image ["!" "[" "]" "(" ")"] @punctuation.delimiter)
(inline_link ["[" "]" "(" ")"] @punctuation.delimiter)
(shortcut_link ["[" "]"] @punctuation.delimiter)

; NOTE: extension not enabled by default
; (wiki_link ["[" "|" "]"] @punctuation.delimiter)
