; Natamux outline patterns for JSON and JSONC: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; A pair, named by the text of its key.
(pair
  key: (string (string_content) @title)) @entry
