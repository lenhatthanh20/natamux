; Natamux outline patterns for Rust: Natamux's own file, in the capture names of
; text_view/outline.rs. The facts (which nodes are items, which words show) come from the
; outline golden files (T-OUT-1, the old editor's output) and the grammar's node names.
; @entry: the item. @title: shown, and part of the name. @word: shown only.
; @body_start, @body_end: the tokens around the body.

; A keyword and a name, after an optional visibility.
[
  (const_item (visibility_modifier)? @word "const" @word name: (_) @title)
  (static_item (visibility_modifier)? @word "static" @word name: (_) @title)
  (type_item (visibility_modifier)? @word "type" @word name: (_) @title)
  (struct_item (visibility_modifier)? @word "struct" @word name: (_) @title)
  (union_item (visibility_modifier)? @word "union" @word name: (_) @title)
  (enum_item (visibility_modifier)? @word "enum" @word name: (_) @title)
  (trait_item (visibility_modifier)? @word "trait" @word name: (_) @title)
  (mod_item (visibility_modifier)? @word "mod" @word name: (_) @title)
] @entry

; Members and macros: the name, after a visibility or "macro_rules!".
[
  (enum_variant name: (_) @title)
  (field_declaration (visibility_modifier)? @word name: (_) @title)
  (macro_definition "macro_rules!" @word name: (_) @title)
] @entry

; "impl", the trait and "for" when there is a trait, then the type.
(impl_item "impl" @word trait: (_)? @title "for"? @word type: (_) @title
  body: (declaration_list "{" @body_start "}" @body_end)) @entry

; Functions, with a body or without: visibility, words such as async, "fn", the name.
[
  (function_item (visibility_modifier)? @word (function_modifiers)? @word "fn" @word
    name: (_) @title body: (block "{" @body_start "}" @body_end))
  (function_signature_item (visibility_modifier)? @word (function_modifiers)? @word "fn" @word
    name: (_) @title)
] @entry
