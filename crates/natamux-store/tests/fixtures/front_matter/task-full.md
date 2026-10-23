---
id: T-0042
title: Fix timezone bug in reminders
state: running
priority: high
labels:
  - bug
  - 'needs: review'
  - '1'
agent: claude-code
branch: natamux/t-0042-fix-timezone
workspace: null
pr: 'https://github.com/o/r/pull/7'
prStatus:
  state: open
  checks: passing
dependsOn:
  - T-0007
parentId: null
repo: null
stage: build
stages:
  - context
  - id: build
    parallel:
      - test-spec
      - implement
  - id: kind
    paths:
      - id: bug
        stages:
          - repro
      - id: feature
        stages:
          - plan
  - release
lanes:
  test-spec: running
  implement: review
paths:
  kind: ''
  other: bug
stageSince: '2026-10-08T09:10:00.000Z'
backgroundSince: '2026-10-08T09:20:00.000Z'
workflowStages:
  - implement
context:
  source: github-issue
  name: Issues
  ref: '#42'
issue:
  provider: github
  key: '#42'
  url: 'https://github.com/o/r/issues/42'
createdAt: '2026-10-08T09:00:00.000Z'
updatedAt: '2026-10-08T09:30:00.000Z'
ciStages:
  - implement
skipStages: []
withoutAgent: true
---
Line one.

- a list

---

After a rule.
