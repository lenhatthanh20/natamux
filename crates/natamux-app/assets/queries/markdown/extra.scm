; Natamux's own patterns for this language, written from the grammar's node-types.json.
; Later patterns win over earlier ones and over the vendored file.

[(atx_heading) (setext_heading) (setext_h1_underline) (setext_h2_underline) (thematic_break)] @title
[
  (list_marker_plus) (list_marker_minus) (list_marker_star) (list_marker_dot)
  (list_marker_parenthesis)
] @punctuation.list_marker
[(block_quote_marker) (block_continuation)] @punctuation.markup
(fenced_code_block_delimiter) @punctuation
(info_string (language) @punctuation)
(link_reference_definition) @link_text
(link_destination) @link_uri
"|" @punctuation.markup
(pipe_table_delimiter_cell "-" @punctuation.markup)
[
  (atx_h1_marker) (atx_h2_marker) (atx_h3_marker) (atx_h4_marker) (atx_h5_marker)
  (atx_h6_marker)
] @title
