; Natamux outline patterns for YAML: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; A mapping pair: the key, and a plain value after it when there is one.
(block_mapping_pair
  key: (_) @title
  value: (flow_node (plain_scalar (string_scalar) @word))?) @entry
