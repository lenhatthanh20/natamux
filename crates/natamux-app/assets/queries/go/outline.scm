; Natamux's own outline patterns, written from the grammar's node-types.json and the old editor's
; outline (golden files T-OUT-1). Captures: @item, @name, @context, @open, @close.

(type_declaration
  "type" @context
  (type_spec
    name: (_) @name) @item)

(field_declaration
  name: (field_identifier) @name @item)

(var_declaration
  "var" @context
  (var_spec
    name: (identifier) @name @item))

(const_declaration
  "const" @context
  (const_spec
    name: (identifier) @name @item))

(function_declaration
  "func" @context
  name: (_) @name) @item

(method_declaration
  "func" @context
  receiver: (parameter_list) @context
  name: (_) @name) @item
