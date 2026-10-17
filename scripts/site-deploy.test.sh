#!/usr/bin/env bash
# Usage: scripts/site-deploy.test.sh
# Checks that .github/workflows/site.yml keeps the site deploy key out of the job that runs
# pnpm install: package scripts there can set BASH_ENV for later steps of the same job.
# Needs only grep and awk, so it runs anywhere.
set -u
cd "$(dirname "$0")/.."
fails=0
workflow=.github/workflows/site.yml

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

# The lines of job $1, without comments.
job() {
  awk -v name="  $1:" '$0 == name { on = 1; next } on && /^  [^ #]/ { on = 0 } on' "$workflow" |
    grep -v '^ *#'
}

check "build job uses no secret" "! job build | grep -q 'secrets\.'"
check "build job uploads site-dist on every event" \
  "job build | grep -q 'name: site-dist' && ! grep -q \"event_name == 'pull_request'\" $workflow"
check "deploy job gets the deploy key" "job deploy | grep -q 'secrets.PAGES_DEPLOY_SSH'"
check "deploy key is used in the deploy job only" \
  "[ \"\$(grep -c PAGES_DEPLOY_SSH $workflow)\" = \"\$(job deploy | grep -c PAGES_DEPLOY_SSH)\" ]"
check "deploy job downloads site-dist" \
  "job deploy | grep -q 'actions/download-artifact@' && job deploy | grep -q 'name: site-dist'"
check "deploy job runs no repo or package code" \
  "! job deploy | grep -Eq 'actions/checkout|setup-node|pnpm|cache'"
check "deploy job waits for build and skips pull requests" \
  "job deploy | grep -q 'needs: build' && job deploy | grep -q \"if: github.event_name != 'pull_request'\""

[ "$fails" = 0 ] && echo "all site deploy checks passed" || echo "$fails site deploy checks failed"
exit "$fails"
