; Vendored from omertuc/tree-sitter-go-work (fork lenhatthanh20/tree-sitter-go-work @acb0617), queries/highlights.scm.
; MIT License, Copyright (c) 2021 Omer Tuchfeld, Copyright (c) 2021 Camden Cheek. Full text in ../LICENSES.
; Changes: none.

[
  "replace"
  "go"
  "use"
] @keyword

"=>" @operator

(comment) @comment

[
(version)
(go_version)
] @string
