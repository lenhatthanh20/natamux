; Natamux outline patterns for JavaScript (parsed with the TSX grammar): Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.

; Classes and their fields.
[
  (class_declaration "class" @word name: (_) @title)
  (public_field_definition "static"? @word name: (_) @title)
] @entry

; Functions and methods show "( )" after the name.
[
  (function_declaration "async"? @word "function" @word name: (_) @title
    parameters: (formal_parameters "(" @word ")" @word))
  (generator_function_declaration "async"? @word "function" @word "*" @word name: (_) @title
    parameters: (formal_parameters "(" @word ")" @word))
  (method_definition "static"? @word "async"? @word ["get" "set"]? @word "*"? @word
    name: (_) @title parameters: (formal_parameters "(" @word ")" @word))
] @entry

; Variables: one entry per name. let and const count only at the top, in a block or after
; export (not in the head of a for loop).
[
  (program (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (identifier) @title) @entry))
  (statement_block (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (identifier) @title) @entry))
  (export_statement (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (identifier) @title) @entry))
  (program (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (array_pattern (identifier) @title @entry))))
  (statement_block (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (array_pattern (identifier) @title @entry))))
  (export_statement (lexical_declaration ["let" "const"] @word
    (variable_declarator name: (array_pattern (identifier) @title @entry))))
  (variable_declaration "var" @word (variable_declarator name: (identifier) @title) @entry)
]

; Object keys: the text of a string key, else the key itself.
(pair key: [
  (string (string_fragment) @title)
  (property_identifier) @title
  (number) @title
  (computed_property_name) @title
]) @entry
