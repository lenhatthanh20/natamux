; Vendored from camdencheek/tree-sitter-go-mod (fork lenhatthanh20/tree-sitter-go-mod @2e88687), queries/highlights.scm.
; MIT License, Copyright (c) 2021 Camden Cheek. Full text in ../LICENSES.
; Changes: none.

[
  "require"
  "replace"
  "go"
  "toolchain"
  "exclude"
  "retract"
  "module"
] @keyword

"=>" @operator

(comment) @comment

[
(version)
(go_version)
] @string
