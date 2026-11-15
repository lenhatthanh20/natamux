; Natamux's own outline patterns, written from the grammar's node-types.json and the old editor's
; outline (golden files T-OUT-1). Captures: @item, @name, @context, @open, @close.

(block_mapping_pair
  key: (_) @name
  value: (flow_node (plain_scalar (string_scalar) @context))?) @item
