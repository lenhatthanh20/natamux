#!/usr/bin/env bash
# A stand-in for Claude Code in the development setup, where claude is not installed.
# scripts/dev.sh seed starts the daemon with it. It prints steps, edits a notes file in the
# workspace, and reports to Helmsman through the same hooks that Claude Code calls.
#
# The prompt picks what it does: it starts with "loop:", "ask:", "fail:" or nothing.
#   loop  keeps working until it is stopped
#   ask   works, asks a question, waits for an answer, then ends its turn
#   fail  works a little, then fails
#   none  works, then ends its turn
set -u

# Helmsman starts it as: <this file> --settings <file> [prompt]
prompt=""
if [ "$#" -ge 3 ]; then
  prompt="${*: -1}"
fi
mode=finish
case "$prompt" in
loop:* | ask:* | fail:*)
  mode="${prompt%%:*}"
  prompt="${prompt#*:}"
  prompt="${prompt# }"
  ;;
esac
notes="notes/${HELMSMAN_TASK_ID:-task}.md"

json() {
  local text=${1//\\/\\\\}
  printf '"%s"' "${text//\"/\\\"}"
}

hook() {
  printf '{"hook_event_name":%s,"cwd":%s%s}' "$(json "$1")" "$(json "$PWD")" "${2:-}" |
    helmsman hook claude-code >/dev/null 2>&1 || true
}

tool() {
  local name=$1 key=$2 value=$3
  hook PreToolUse ",\"tool_name\":$(json "$name"),\"tool_input\":{$(json "$key"):$(json "$value")}"
  if [ "$key" = command ]; then
    printf '\e[90m●\e[0m \e[1m%s\e[0m(%s)\n' "$name" "$value"
  else
    printf '\e[90m●\e[0m \e[1m%s\e[0m %s\n' "$name" "${value#"$PWD"/}"
  fi
}

say() {
  printf '%s\n\n' "$1"
}

pause() {
  sleep "$((RANDOM % 4 + 2))"
}

# One unit of work: read, edit the notes file, run the tests.
work() {
  local round=$1
  tool Read file_path "$PWD/README.md"
  pause
  mkdir -p notes
  printf -- '- Step %s for: %s\n' "$round" "$prompt" >>"$notes"
  tool Edit file_path "$PWD/$notes"
  printf '  \e[32m+1 line\e[0m\n'
  pause
  tool Bash command "pnpm --filter @helmsman/web test"
  pause
  printf '  \e[32m✓ %s passed\e[0m\n\n' "$((120 + round))"
}

printf '\e[2m╭──────────────────────────────────────────────╮\e[0m\n'
printf '\e[2m│\e[0m \e[1mFake agent\e[0m, a stand-in for Claude Code in dev \e[2m│\e[0m\n'
printf '\e[2m╰──────────────────────────────────────────────╯\e[0m\n\n'
printf '\e[1m>\e[0m %s\n\n' "$prompt"
hook UserPromptSubmit

round=1
case "$mode" in
loop)
  while true; do
    work "$round"
    round=$((round + 1))
  done
  ;;
ask)
  work 1
  work 2
  question="Keep the old ticket format for one release, or drop it now?"
  printf '\e[33m?\e[0m %s\n' "$question"
  hook Notification ",\"notification_type\":\"permission_prompt\",\"message\":$(json "$question")"
  printf '\e[2m(type an answer and press Enter)\e[0m\n> '
  read -r answer
  # A message from the dashboard arrives as a bracketed paste.
  answer="${answer//$'\e[200~'/}"
  answer="${answer//$'\e[201~'/}"
  hook UserPromptSubmit
  say "Got it: $answer"
  work 3
  say "Done. The change is ready for review."
  helmsman done >/dev/null 2>&1 || true
  hook Stop
  ;;
fail)
  work 1
  tool Bash command "pnpm --filter @helmsman/daemon test"
  printf '  \e[31m✗ 2 failed: git.test.ts › retries a push after a network error\e[0m\n\n'
  say "The tests fail and I cannot fix them without more context. Stopping."
  hook StopFailure
  exit 1
  ;;
*)
  work 1
  work 2
  say "Done. The change is ready for review."
  helmsman done >/dev/null 2>&1 || true
  hook Stop
  ;;
esac
