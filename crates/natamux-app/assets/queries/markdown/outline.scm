; Natamux outline patterns for Markdown: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; A section: its heading marker, then the heading text.
(section
  (atx_heading
    [
      (atx_h1_marker)
      (atx_h2_marker)
      (atx_h3_marker)
      (atx_h4_marker)
      (atx_h5_marker)
      (atx_h6_marker)
    ] @word
    heading_content: (_) @title)) @entry
