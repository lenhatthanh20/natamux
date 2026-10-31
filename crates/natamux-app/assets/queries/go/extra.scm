; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

(package_identifier) @namespace
(const_spec name: (identifier) @constant)
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "." ";" ":"] @punctuation.delimiter
(escape_sequence) @string.escape
