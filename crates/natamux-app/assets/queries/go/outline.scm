; Natamux outline patterns for Go: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; Package level: one entry per spec, named by the spec's name.
(type_declaration "type" @word (type_spec name: (_) @title) @entry)
(var_declaration "var" @word (var_spec name: (identifier) @title @entry))
(const_declaration "const" @word (const_spec name: (identifier) @title) @entry)

; Struct fields.
(field_declaration name: (field_identifier) @title @entry)

; Functions; a method also shows its receiver list.
[
  (function_declaration "func" @word name: (_) @title)
  (method_declaration "func" @word receiver: (parameter_list) @word name: (_) @title)
] @entry
