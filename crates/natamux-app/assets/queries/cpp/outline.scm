; Natamux outline patterns for C++. Worked out from the old editor's output (golden files T-OUT-1)
; and node names as the author remembers them; no grammar file was at hand.
; Captures: @item the entry, @name shown and a name, @context shown only, @open/@close body tokens.

(concept_definition "concept" @context name: (_) @name) @item
(namespace_definition "namespace" @context name: (_) @name) @item
(class_specifier "class" @context name: (_) @name body: (_)) @item
(enum_specifier "enum" @context "class"? @context name: (_) @name body: (_)) @item

; Preprocessor definitions.
(preproc_def "#define" @context name: (_) @name) @item
(preproc_function_def "#define" @context name: (_) @name
  parameters: (preproc_params "(" @context ")" @context)) @item

(type_definition "typedef" @context declarator: (_) @name) @item

; Types with a body.
(struct_specifier "struct" @context name: (_) @name body: (_)) @item
(union_specifier "union" @context name: (_) @name body: (_)) @item
(enumerator name: (_) @name) @item
(field_declaration type: (_) @context declarator: (field_identifier) @name @item)

; Functions: storage word, return type, then the declarator's name and "( )".
(function_definition
  (storage_class_specifier)? @context
  type: (_) @context
  declarator: (function_declarator
    declarator: (_) @name
    parameters: (parameter_list "(" @context ")" @context))) @item

; Variables at any level.
(declaration (storage_class_specifier)? @context type: (_) @context
  declarator: (init_declarator declarator: (_) @name) @item)
(declaration (storage_class_specifier)? @context type: (_) @context
  declarator: (identifier) @name @item)
