; Natamux outline patterns for Python: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; Classes and functions: the keywords, then the name.
[
  (class_definition "class" @word name: (_) @title)
  (function_definition "async"? @word "def" @word name: (_) @title)
] @entry
