; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

(word) @string
(command_name (word) @function)
(function_definition name: (word) @function)
((command argument: (_) @constant) (#match? @constant "^-"))
(case_item value: (word) @string.regex)
[(regex) (extglob_pattern)] @string.regex
(variable_name) @variable
(special_variable_name) @variable.special
[(heredoc_start) (heredoc_content) (heredoc_end)] @string
(number) @number
(test_operator) @keyword
["declare" "local"] @keyword
((comment) @keyword (#match? @keyword "^#!"))

[
  "!" "&&" "+" "+=" "-" "<" "<<" "=" "=~" ">&" ">" "|" "||" ">>" "==" "!=" "&>" "<<<" "*" "/"
] @operator
(expansion "#" @operator)
["$" "$(" "$((" "${"] @punctuation.special
(command_substitution ")" @punctuation.special)
(arithmetic_expansion "))" @punctuation.special)
(expansion "}" @punctuation.special)
["(" "((" "[" "[[" "]" "]]" "{"] @punctuation.bracket
(array ")" @punctuation.bracket)
(case_item ")" @punctuation.bracket)
(function_definition ")" @punctuation.bracket)
(process_substitution ")" @punctuation.bracket)
(compound_statement ["))" "}"] @punctuation.bracket)
";" @punctuation.delimiter
[":-" "<("] @embedded
