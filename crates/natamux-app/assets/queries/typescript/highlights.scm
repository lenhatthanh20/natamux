; Vendored from tree-sitter/tree-sitter-typescript (fork lenhatthanh20/tree-sitter-typescript @e2c5359), queries/highlights.scm.
; MIT License, Copyright (c) 2017 Max Brunsfeld. Full text in ../LICENSES.
; Changes: none.

; Types

(type_identifier) @type
(predefined_type) @type.builtin

((identifier) @type
 (#match? @type "^[A-Z]"))

(type_arguments
  "<" @punctuation.bracket
  ">" @punctuation.bracket)

; Variables

(required_parameter (identifier) @variable.parameter)
(optional_parameter (identifier) @variable.parameter)

; Keywords

[ "abstract"
  "declare"
  "enum"
  "export"
  "implements"
  "interface"
  "keyof"
  "namespace"
  "private"
  "protected"
  "public"
  "type"
  "readonly"
  "override"
  "satisfies"
] @keyword
