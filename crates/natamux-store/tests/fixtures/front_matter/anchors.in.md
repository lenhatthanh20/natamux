---
base: &b {x: 1, y: 2}
merged:
  <<: *b
  y: 3
list: [*b]
---
