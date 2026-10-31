; Natamux's own query for .env files, on the bash grammar.

(comment) @comment
(variable_name) @variable
"=" @operator
"export" @keyword
[(string) (raw_string) (word)] @string
(number) @number
(expansion ["${" "}"] @punctuation.special)
":-" @embedded
