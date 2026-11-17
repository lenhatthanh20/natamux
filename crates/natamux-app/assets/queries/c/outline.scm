; Natamux outline patterns for C: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; A keyword and a name; struct, union and enum count only with a body.
[
  (enum_specifier "enum" @word name: (_) @title body: (_))
  (struct_specifier "struct" @word name: (_) @title body: (_))
  (union_specifier "union" @word name: (_) @title body: (_))
  (type_definition "typedef" @word declarator: (_) @title)
  (preproc_def "#define" @word name: (_) @title)
  (preproc_function_def "#define" @word name: (_) @title
    parameters: (preproc_params "(" @word ")" @word))
  (enumerator name: (_) @title)
] @entry

; Struct fields: the type, then the name.
(field_declaration type: (_) @word declarator: (field_identifier) @title @entry)

; Functions: the storage word and the return type, then the declarator's name and "( )".
(function_definition
  (storage_class_specifier)? @word
  type: (_) @word
  declarator: (function_declarator
    declarator: (_) @title
    parameters: (parameter_list "(" @word ")" @word))) @entry

; Variables at any level, one entry per declarator.
(declaration (storage_class_specifier)? @word type: (_) @word
  declarator: [
    (init_declarator declarator: (_) @title) @entry
    (identifier) @title @entry
  ])
