; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

[(true) (false)] @boolean
(null) @constant
["{" "}" "[" "]"] @punctuation.bracket
["," ":"] @punctuation.delimiter
