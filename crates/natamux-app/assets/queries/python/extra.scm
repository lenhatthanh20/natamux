; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

(identifier) @variable
(attribute attribute: (identifier) @property)
(call function: (identifier) @function)
(call function: (attribute attribute: (identifier) @function))
(function_definition name: (identifier) @function)
(keyword_argument name: (identifier) @function)
(parameters (identifier) @variable.parameter)
(typed_parameter (identifier) @variable.parameter)
(typed_default_parameter name: (identifier) @variable.parameter)
(default_parameter name: (identifier) @variable.parameter)
(list_splat_pattern (identifier) @variable.parameter)
(dictionary_splat_pattern (identifier) @variable.parameter)

((identifier) @type (#match? @type "^[A-Z]"))
((identifier) @constant (#match? @constant "^[A-Z][A-Z_\\d]*$"))
(type (identifier) @type)
((identifier) @variable.special (#any-of? @variable.special "self" "cls"))

(none) @constant
[(true) (false)] @boolean
[(integer) (float)] @number
(escape_sequence) @string.escape
(type_conversion) @embedded

["->" ":=" "*" "**" "+" "+=" "=" "==" ">" "|" "-" "/" "//" "%" "<" "<=" ">=" "!=" "-=" "*=" "/=" "&" "^" "~" "<<" ">>" "@"] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "." ":" ";"] @punctuation.delimiter
["and" "or" "not" "in" "is" "is not" "not in"] @keyword
(interpolation ["{" "}"] @punctuation.special)
(format_expression ["{" "}"] @punctuation.bracket)

; Calls of an attribute, builtin types and decorators.
(call function: (attribute attribute: (identifier) @function))
((call function: (identifier) @type)
 (#any-of? @type "bool" "bytearray" "bytes" "complex" "dict" "float" "frozenset" "int" "list" "range" "set" "str" "tuple"))
(generic_type (identifier) @type)
(decorator "@" @function)
(decorator (identifier) @function)
((decorator "@" @attribute (identifier) @attribute.name)
 (#any-of? @attribute.name "property" "staticmethod" "classmethod"))
