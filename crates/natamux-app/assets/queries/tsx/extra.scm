; Natamux's own JSX patterns, written from the grammar's node-types.json. JavaScript and TSX
; both use them (the JavaScript language parses with the TSX grammar).

(jsx_opening_element name: (identifier) @tag)
(jsx_closing_element name: (identifier) @tag)
(jsx_self_closing_element name: (identifier) @tag)
(jsx_attribute (property_identifier) @attribute)
(jsx_opening_element ["<" ">"] @punctuation.bracket)
(jsx_closing_element ["</" ">"] @punctuation.bracket)
(jsx_self_closing_element ["<" "/>"] @punctuation.bracket)
(jsx_attribute "=" @punctuation.delimiter)
(html_character_reference) @string.special
