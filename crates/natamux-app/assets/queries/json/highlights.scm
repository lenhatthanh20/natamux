; Vendored from tree-sitter/tree-sitter-json v0.24.8, queries/highlights.scm.
; MIT License, Copyright (c) 2014 Max Brunsfeld. Full text in ../LICENSES.
; Changes: none.

(pair
  key: (_) @string.special.key)

(string) @string

(number) @number

[
  (null)
  (true)
  (false)
] @constant.builtin

(escape_sequence) @escape

(comment) @comment
