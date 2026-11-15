; Natamux outline patterns for TypeScript and TSX. Worked out from the old editor's output (golden files T-OUT-1)
; and node names as the author remembers them; no grammar file was at hand.
; Captures: @item the entry, @name shown and a name, @context shown only, @open/@close body tokens.

(internal_module "namespace" @context name: (_) @name) @item
(enum_declaration "enum" @context name: (_) @name) @item
(type_alias_declaration "type" @context name: (_) @name) @item
(interface_declaration "interface" @context name: (_) @name) @item
(abstract_class_declaration "abstract" @context "class" @context name: (_) @name) @item
(public_field_definition
  (accessibility_modifier)? @context
  "static"? @context
  "readonly"? @context
  name: (_) @name) @item

; Declarations that are a keyword and a name.
(class_declaration "class" @context name: (_) @name) @item
(function_declaration "async"? @context "function" @context name: (_) @name
  parameters: (formal_parameters "(" @context ")" @context)) @item
(generator_function_declaration "async"? @context "function" @context "*" @context name: (_) @name
  parameters: (formal_parameters "(" @context ")" @context)) @item

; Members of a class. A method shows its modifiers, then the name and "( )".
(method_definition
  (accessibility_modifier)? @context
  "static"? @context
  "async"? @context
  ["get" "set"]? @context
  "*"? @context
  name: (_) @name
  parameters: (formal_parameters "(" @context ")" @context)) @item

; Variables: the name of a declarator, or the names in an array pattern.
(lexical_declaration ["let" "const"] @context
  (variable_declarator name: (identifier) @name) @item)
(lexical_declaration ["let" "const"] @context
  (variable_declarator name: (array_pattern (identifier) @name @item)))
(variable_declaration "var" @context
  (variable_declarator name: (identifier) @name) @item)

; Keys of an object literal.
(pair key: (string (string_fragment) @name)) @item
(pair key: [(property_identifier) (number) (computed_property_name)] @name) @item
