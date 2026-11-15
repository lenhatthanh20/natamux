; Natamux's own outline patterns, written from the grammar's node-types.json and the old editor's
; outline (golden files T-OUT-1). Captures: @item, @name, @context, @open, @close.

(preproc_def
  "#define" @context
  name: (_) @name) @item

(preproc_function_def
  "#define" @context
  name: (_) @name
  parameters: (preproc_params "(" @context ")" @context)) @item

(type_definition
  "typedef" @context
  declarator: (_) @name) @item

(namespace_definition
  "namespace" @context
  name: (_) @name) @item

(struct_specifier
  "struct" @context
  name: (_) @name
  body: (_)) @item

(class_specifier
  "class" @context
  name: (_) @name
  body: (_)) @item

(union_specifier
  "union" @context
  name: (_) @name
  body: (_)) @item

(enum_specifier
  "enum" @context
  "class"? @context
  name: (_) @name
  body: (_)) @item

(enumerator
  name: (_) @name) @item

(field_declaration
  type: (_) @context
  declarator: (field_identifier) @name @item)

(function_definition
  (storage_class_specifier)? @context
  type: (_) @context
  declarator: (function_declarator
    declarator: (_) @name
    parameters: (parameter_list "(" @context ")" @context))) @item

(declaration
  (storage_class_specifier)? @context
  type: (_) @context
  declarator: (init_declarator
    declarator: (_) @name) @item)

(declaration
  (storage_class_specifier)? @context
  type: (_) @context
  declarator: (identifier) @name @item)
