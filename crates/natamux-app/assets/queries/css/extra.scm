; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

(class_selector (class_name) @selector)
(id_name) @selector
(pseudo_class_selector (class_name) @selector.pseudo)
(pseudo_element_selector (tag_name) @selector.pseudo)

(plain_value) @constant
((plain_value) @variable (#match? @variable "^--"))
(keyframes_name) @constant
(keyword_query) @constant
(color_value "#" @string.special)

"and" @keyword
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
[";" "." "::"] @punctuation.delimiter
