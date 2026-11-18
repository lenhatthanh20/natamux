; Natamux outline patterns for CSS: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; A rule set: every selector of its list is part of the name.
(rule_set
  (selectors _ @title)) @entry

; A media query: "@media" and the first query.
(media_statement
  "@media" @word
  .
  (_) @title) @entry
