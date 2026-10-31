; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

; Names: plain ones are variables; some places make them functions or parameters.
(identifier) @variable
(metavariable) @variable
(shorthand_field_identifier) @property

(call_expression function: (identifier) @function)
(call_expression function: (scoped_identifier name: (identifier) @function))
(call_expression function: (field_expression field: (field_identifier) @function))
(generic_function function: (identifier) @function)
(generic_function function: (scoped_identifier name: (identifier) @function))
(generic_function function: (field_expression field: (field_identifier) @function))
(function_item name: (identifier) @function)
(function_signature_item name: (identifier) @function)
(macro_definition name: (identifier) @function)
(macro_invocation macro: (identifier) @function "!" @function)
(parameter pattern: (identifier) @variable.parameter)
(attribute (identifier) @attribute.name)
(attribute arguments: (token_tree (identifier) @attribute.name))

; Names in capitals are types, and all-capital names are constants.
((identifier) @type (#match? @type "^[A-Z]"))
((identifier) @constant (#match? @constant "^[A-Z][A-Z\\d_]*$"))

(fragment_specifier) @type

; Literals and comments.
(integer_literal) @number
(float_literal) @number
(boolean_literal) @boolean
(escape_sequence) @string.escape
(line_comment (doc_comment)) @comment.doc
(block_comment (doc_comment)) @comment.doc

; Operators and punctuation.
[
  "%" "&&" "&" "*" "+" "+=" "-=" "->" ".." "..=" ":" "<" "=" "==" "=>" ">" "?" "|"
  "!=" "-" "/" "<=" ">=" "||" "^" "<<" ">>" "*=" "/=" "%=" "&=" "|=" "^=" "<<=" ">>="
] @operator
"#" @punctuation.special
(lifetime "'" @none)
(outer_doc_comment_marker) @comment.doc
(inner_doc_comment_marker) @comment.doc
