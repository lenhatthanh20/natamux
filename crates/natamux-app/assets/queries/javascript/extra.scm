; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.
; This file is shared by JavaScript, TypeScript and TSX.

(identifier) @variable
[(property_identifier) (private_property_identifier) (shorthand_property_identifier)] @property

(function_declaration name: (identifier) @function)
(method_definition name: (property_identifier) @function)
(variable_declarator name: (identifier) @function value: [(function_expression) (arrow_function)])
(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: [(property_identifier) (private_property_identifier)] @function))

((identifier) @type
 (#any-of? @type
  "Array" "ArrayBuffer" "BigInt" "Boolean" "DataView" "Date" "Error" "EvalError" "Function"
  "Map" "Number" "Object" "Promise" "Proxy" "RangeError" "ReferenceError" "RegExp" "Set"
  "String" "Symbol" "SyntaxError" "TypeError" "URIError" "WeakMap" "WeakSet"))
(new_expression constructor: (identifier) @type)
((identifier) @constant (#match? @constant "^[A-Z_][A-Z\\d_]+$"))
(call_expression function: (identifier) @function)

(required_parameter pattern: (identifier) @variable.parameter)
(optional_parameter pattern: (identifier) @variable.parameter)
(required_parameter value: (member_expression object: (identifier) @variable.parameter))
(formal_parameters
  (required_parameter
    pattern: (object_pattern (shorthand_property_identifier_pattern) @variable.parameter)))

(this) @variable.special
[(null) (undefined)] @constant
[(true) (false)] @boolean
(number) @number
(escape_sequence) @string.escape

[
  "!" "&&" "+" "++" "..." "<" "=" "=>" ">" ">=" "?" "??" "-" "--" "*" "**" "/" "%" "<=" "==" "==="
  "!=" "!==" "||" "+=" "-=" "*=" "/=" "&" "|" "^" "~" "<<" ">>" ">>>" "??=" "||=" "&&="
] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "." ";" ":" "?."] @punctuation.delimiter
(ternary_expression ":" @operator)
(template_substitution ["${" "}"] @punctuation.special)
(regex "/" @string.regex)
(regex_pattern) @string.regex
(regex_flags) @keyword

; Types (the TypeScript and TSX grammars; the JavaScript language parses with TSX).
(type_identifier) @type
(type_annotation) @type
(type_annotation ":" @punctuation.special)
(type_predicate_annotation ":" @punctuation.special)
(union_type "|" @punctuation.special)
(type_parameters ["<" ">"] @punctuation.bracket)
(type_arguments ["<" ">"] @punctuation.bracket)
(type_predicate name: (identifier) @variable.parameter "is" @keyword)
(required_parameter name: (identifier) @variable.parameter)
(optional_parameter name: (identifier) @variable.parameter)
(property_signature "?" @punctuation.special)
(public_field_definition "?" @punctuation.special)
(optional_parameter "?" @punctuation.special)
(predefined_type "void" @keyword)
(decorator "@" @punctuation.special)
(method_definition name: (property_identifier) @constructor (#eq? @constructor "constructor"))
