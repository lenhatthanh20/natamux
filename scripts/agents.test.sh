#!/usr/bin/env bash
# Usage: scripts/agents.test.sh
# Checks the agent rule files: AGENTS.md holds the rules for every agent, and CLAUDE.md only
# imports it, so the two never drift apart.
set -u
cd "$(dirname "$0")/.."
fails=0

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

check "AGENTS.md has the rules" 'grep -q "^# AGENTS.md" AGENTS.md && grep -q "^## Rules" AGENTS.md'
check "AGENTS.md lists the skills" 'grep -q "^## Skills" AGENTS.md'
check "CLAUDE.md only imports AGENTS.md" '[ "$(cat CLAUDE.md)" = "@AGENTS.md" ]'
check "no product names in the rule files" '! grep -qiwE "orca|atlas|superset|emdash" AGENTS.md CLAUDE.md'
check "docs point to AGENTS.md for the rules" '! grep -q "CLAUDE\.md" docs/research-native-app.md docs/ui-spec.md'

[ "$fails" -eq 0 ]
