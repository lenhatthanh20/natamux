; Natamux's own patterns for TypeScript only, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

; A function stored in a variable takes the function colour where nothing else colours it.
(variable_declarator value: (arrow_function) @function)
(arrow_function parameters: (formal_parameters ["(" ")"] @punctuation.bracket))
(arrow_function type_parameters: (type_parameters ["<" ">"] @punctuation.bracket))
(arrow_function parameter: (identifier) @variable.parameter)
(arrow_function "async" @keyword)
(import_specifier "type" name: (identifier) @type)
