; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

; Names.
(identifier) @variable
(field_identifier) @property
(statement_identifier) @label
(namespace_identifier) @namespace

(call_expression function: (identifier) @function)
(call_expression function: (field_expression field: (field_identifier) @function))
(call_expression function: (qualified_identifier name: (identifier) @function))
(function_declarator declarator: (identifier) @function)
(function_declarator declarator: (field_identifier) @function)
(function_declarator declarator: (qualified_identifier name: (identifier) @function))
(template_function name: (identifier) @function)
(template_method name: (field_identifier) @function)
(destructor_name (identifier) @function)
(operator_name "[]" @function)
(attribute name: (identifier) @attribute)

; Capital names: namespaces that start with one are types, all-capital names are constants.
((namespace_identifier) @type (#match? @type "^[A-Z]"))
((identifier) @constant (#match? @constant "^[A-Z][A-Z\\d_]*$"))

["goto" "operator"] @keyword

; Types, literals, comments.
[(primitive_type) (type_identifier) (auto) (sized_type_specifier)] @type
(number_literal) @number
[(true) (false)] @boolean
(null) @constant
[(string_literal) (char_literal) (raw_string_literal) (system_lib_string)] @string
(escape_sequence) @string.escape
(comment) @comment

; The preprocessor.
["#define" "#elif" "#else" "#endif" "#if" "#ifdef" "#ifndef" "#include"] @preproc
(preproc_directive) @preproc
(preproc_arg) @none

; Operators and punctuation.
[
  "!" "%" "&&" "&" "*" "+" "++" "+=" "-" "-=" "->" "." "<" "<<" "<=" "=" "==" ">" "?" "~"
  "!=" "--" "/" ">=" ">>" "||" "^" "|" "*=" "/=" "%=" "&=" "|=" "^=" "<<=" ">>="
] @operator
[";" "," ":" "::"] @punctuation.delimiter
(conditional_expression ":" @operator)
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["..." "[[" "]]" "mutable"] @none
