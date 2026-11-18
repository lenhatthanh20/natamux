; Natamux outline patterns for TypeScript and TSX: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; A keyword and a name.
[
  (internal_module "namespace" @word name: (_) @title)
  (enum_declaration "enum" @word name: (_) @title)
  (type_alias_declaration "type" @word name: (_) @title)
  (interface_declaration "interface" @word name: (_) @title)
  (class_declaration "class" @word name: (_) @title)
  (abstract_class_declaration "abstract" @word "class" @word name: (_) @title)
] @entry

; Class fields: their modifiers, then the name.
(public_field_definition (accessibility_modifier)? @word "static"? @word "readonly"? @word
  name: (_) @title) @entry

; Functions and methods show "( )" after the name.
[
  (function_declaration "async"? @word "function" @word name: (_) @title
    parameters: (formal_parameters "(" @word ")" @word))
  (generator_function_declaration "async"? @word "function" @word "*" @word name: (_) @title
    parameters: (formal_parameters "(" @word ")" @word))
  (method_definition (accessibility_modifier)? @word "static"? @word "async"? @word ["get" "set"]? @word "*"? @word
    name: (_) @title parameters: (formal_parameters "(" @word ")" @word))
] @entry

; Variables: one entry per name.
[
  (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (identifier) @title) @entry)
  (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (array_pattern (identifier) @title @entry)))
  (variable_declaration "var" @word (variable_declarator name: (identifier) @title) @entry)
]

; Object keys: the text of a string key, else the key itself.
(pair key: [
  (string (string_fragment) @title)
  (property_identifier) @title
  (number) @title
  (computed_property_name) @title
]) @entry
