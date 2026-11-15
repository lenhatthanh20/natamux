; Natamux outline patterns for Go. Worked out from the old editor's output (golden files T-OUT-1)
; and node names as the author remembers them; no grammar file was at hand.
; Captures: @item the entry, @name shown and a name, @context shown only, @open/@close body tokens.

; Package level: one entry per spec, named by the spec's name.
(type_declaration "type" @context (type_spec name: (_) @name) @item)
(var_declaration "var" @context (var_spec name: (identifier) @name @item))
(const_declaration "const" @context (const_spec name: (identifier) @name @item))

; Struct fields.
(field_declaration name: (field_identifier) @name @item)

; Functions; a method also shows its receiver list.
(function_declaration "func" @context name: (_) @name) @item
(method_declaration "func" @context receiver: (parameter_list) @context name: (_) @name) @item
