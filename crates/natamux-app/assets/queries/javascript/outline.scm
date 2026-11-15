; Natamux's own outline patterns, written from the grammar's node-types.json and the old editor's
; outline (golden files T-OUT-1). Captures: @item, @name, @context, @open, @close.

(class_declaration
  "class" @context
  name: (_) @name) @item

(function_declaration
  "async"? @context
  "function" @context
  name: (_) @name
  parameters: (formal_parameters "(" @context ")" @context)) @item

(generator_function_declaration
  "async"? @context
  "function" @context
  "*" @context
  name: (_) @name
  parameters: (formal_parameters "(" @context ")" @context)) @item

(method_definition
  "static"? @context
  "async"? @context
  ["get" "set"]? @context
  "*"? @context
  name: (_) @name
  parameters: (formal_parameters "(" @context ")" @context)) @item

(field_definition
  "static"? @context
  property: (_) @name) @item

(lexical_declaration
  ["let" "const"] @context
  (variable_declarator
    name: (identifier) @name) @item)

(lexical_declaration
  ["let" "const"] @context
  (variable_declarator
    name: (array_pattern (identifier) @name @item)))

(variable_declaration
  "var" @context
  (variable_declarator
    name: (identifier) @name) @item)

(pair
  key: (_) @name) @item
