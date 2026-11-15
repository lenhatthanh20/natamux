; Natamux's own outline patterns, written from the grammar's node-types.json and the old editor's
; outline (golden files T-OUT-1). Captures: @item, @name, @context, @open, @close.

(class_definition
  "class" @context
  name: (_) @name) @item

(function_definition
  "async"? @context
  "def" @context
  name: (_) @name) @item
