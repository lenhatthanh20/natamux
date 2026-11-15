; Natamux's own outline patterns, written from the grammar's node-types.json and the old editor's
; outline (golden files T-OUT-1). Captures: @item, @name, @context, @open, @close.

(rule_set
  (selectors _ @name)) @item

(media_statement
  "@media" @context
  .
  (_) @name) @item
