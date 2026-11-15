; Natamux outline patterns. Worked out from the old editor output (golden files T-OUT-1)
; and node names as the author remembers them; no grammar file was at hand.

(rule_set
  (selectors _ @name)) @item

(media_statement
  "@media" @context
  .
  (_) @name) @item
