; Natamux outline patterns for Rust. Worked out from the old editor's output (golden files T-OUT-1)
; and node names as the author remembers them; no grammar file was at hand.
; Captures: @item the entry, @name shown and a name, @context shown only, @open/@close body tokens.

; Values.
(const_item (visibility_modifier)? @context "const" @context name: (_) @name) @item
(static_item (visibility_modifier)? @context "static" @context name: (_) @name) @item
(type_item (visibility_modifier)? @context "type" @context name: (_) @name) @item
(macro_definition "macro_rules!" @context name: (_) @name) @item

; Types and their members.
(struct_item (visibility_modifier)? @context "struct" @context name: (_) @name) @item
(union_item (visibility_modifier)? @context "union" @context name: (_) @name) @item
(enum_item (visibility_modifier)? @context "enum" @context name: (_) @name) @item
(enum_variant name: (_) @name) @item
(field_declaration (visibility_modifier)? @context name: (_) @name) @item

; The shown text of an impl is "impl", an optional trait, "for", and the type.
(impl_item
  "impl" @context
  trait: (_)? @name
  "for"? @context
  type: (_) @name
  body: (declaration_list "{" @open "}" @close)) @item

(trait_item (visibility_modifier)? @context "trait" @context name: (_) @name) @item
(mod_item (visibility_modifier)? @context "mod" @context name: (_) @name) @item

; Functions: the word list before "fn" is visibility, then async/unsafe and the like.
(function_item
  (visibility_modifier)? @context
  (function_modifiers)? @context
  "fn" @context
  name: (_) @name
  body: (block "{" @open "}" @close)) @item

(function_signature_item
  (visibility_modifier)? @context
  (function_modifiers)? @context
  "fn" @context
  name: (_) @name) @item
