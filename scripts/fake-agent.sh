#!/usr/bin/env bash
# A stand-in for Claude Code in the E2E image, where claude is not installed.
# compose.e2e.yaml starts the daemon with it. It prints steps, edits a notes file in the
# workspace, and reports to Natamux through the same hooks that Claude Code calls.
#
# A stage prompt (it ends with "run `natamux done`") is done for real, roughly: the files the
# prompt names are written with the sections it names, code stages change code and commit when the
# prompt asks for it, and failed checks that come back are fixed. Then it waits for the next
# prompt, as Claude Code does. Marks in the task title make it go wrong on purpose, for tests:
#   [miss-section]  leaves out the last section the first time
#   [no-commit]     does not commit the first time, even when the prompt asks for it
#   [no-done]       ends its turn without `natamux done`
#   [ask]           asks a question and waits for the answer first
#
# Any other prompt picks what it does: it starts with "loop:", "ask:", "fail:" or nothing.
#   loop  keeps working until it is stopped
#   ask   works, asks a question, waits for an answer, then ends its turn
#   fail  works a little, then fails
#   none  works, then ends its turn
set -u

# A one-shot run, such as the commit message draft: the prompt comes on stdin, one answer goes
# to stdout. A task title with [no-draft] makes it fail, so the simple draft shows.
if [ "${1:-}" = -p ]; then
  case "$(cat)" in
  *'[no-draft]'*)
    echo "fake agent: no draft for this task" >&2
    exit 1
    ;;
  esac
  printf 'Add fixed message\n'
  exit 0
fi

# Natamux starts it as: <this file> --settings <file> [--permission-mode <mode>] [prompt]
prompt=""
while [ "$#" -gt 0 ]; do
  case "$1" in
  --settings | --permission-mode) shift 2 || shift ;;
  *)
    prompt="$1"
    shift
    ;;
  esac
done
mode=finish
case "$prompt" in
loop:* | ask:* | fail:*)
  mode="${prompt%%:*}"
  prompt="${prompt#*:}"
  prompt="${prompt# }"
  ;;
esac
notes="notes/${NATAMUX_TASK_ID:-task}.md"

json() {
  local text=${1//\\/\\\\}
  printf '"%s"' "${text//\"/\\\"}"
}

# Claude Code sets CLAUDECODE; `natamux hook claude-code` drops a hook without it.
hook() {
  printf '{"hook_event_name":%s,"cwd":%s%s}' "$(json "$1")" "$(json "$PWD")" "${2:-}" |
    CLAUDECODE=1 natamux hook claude-code >/dev/null 2>&1 || true
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

# The contract job sets FAST: its scenarios wait for the agent, not for the pretend work.
pause() {
  [ "${NATAMUX_FAKE_AGENT_FAST:-}" = 1 ] && return
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
  tool Bash command "pnpm --filter @natamux/web test"
  pause
  printf '  \e[32m✓ %s passed\e[0m\n\n' "$((120 + round))"
}

# The next prompt typed or pasted into the terminal: the first line, then the lines that follow
# at once. A paste from the dashboard comes in bracket marks, which are dropped.
read_prompt() {
  local line text
  IFS= read -r line || return 1
  text=$line
  while IFS= read -r -t 0.5 line; do
    text+=$'\n'$line
  done
  text=${text//$'\e[200~'/}
  text=${text//$'\e[201~'/}
  printf '%s' "${text//$'\r'/}"
}

title_of() {
  local first=${1%%$'\n'*}
  printf '%s' "${first#Task: }"
}

# Writes a stage file with a heading per section, like an agent that followed the prompt.
write_file() {
  local file=$1 sections=$2 skip_last=$3
  mkdir -p "$(dirname "$file")"
  local IFS=','
  read -r -a names <<<"$sections"
  local count=${#names[@]}
  [ "$skip_last" = yes ] && count=$((count - 1))
  {
    printf '# %s\n\n' "$(basename "$file" .md)"
    for ((i = 0; i < count; i++)); do
      local name=${names[$i]# }
      name=${name% }
      printf '## %s\n\n' "$name"
      if [ "$name" = Cases ]; then
        printf '### An empty input gives an empty result\n\n### A title with a comma is quoted\n\n### Dates are in UTC\n\n'
      else
        printf -- '- %s for this task.\n\n' "$name"
      fi
    done
  } >"$file"
  tool Write file_path "$PWD/$file"
}

change_code() {
  local commit=$1
  mkdir -p src test
  local name="feature-$(date +%s)"
  printf 'export const %s = (text) => text.trim()\n' "${name//-/_}" >"src/$name.js"
  printf "import { test } from 'node:test'\ntest('%s', () => {})\n" "$name" >"test/$name.test.js"
  tool Edit file_path "$PWD/src/$name.js"
  if [ "$commit" = yes ]; then
    git add -A src test && git commit -q -m "Add $name" && tool Bash command "git commit -m 'Add $name'"
  fi
}

# Fixes what failed checks name, as the agent would.
fix_checks() {
  local text=$1 line file section
  while IFS= read -r line; do
    case "$line" in
    *"has no case under Cases"*) file=".natamux-task/${line#- }" && file=${file%% has*} &&
      printf '## Cases\n\n### One more case\n\n' >>"$file" ;;
    *" has no "*" section"*)
      file=".natamux-task/${line#- }" && file=${file%% has no*}
      section=${line#* has no } && section=${section% section}
      local IFS=','
      for name in $section; do printf '## %s\n\n- Added.\n\n' "${name# }" >>"$file"; done
      unset IFS
      tool Edit file_path "$PWD/$file" ;;
    *"not committed"* | *"no new commit"*)
      git add -A && git commit -q -m "Commit the rest" || change_code yes ;;
    *" is missing"*) file=${line#- } && file=${file%% is missing} && write_file "$file" "Notes" no ;;
    esac
  done <<<"$text"
}

do_stage() {
  local text=$1 title
  title=$(title_of "$text")
  printf '\e[1m>\e[0m %s\n\n' "${text//$'\n'/$'\n'  }"
  hook UserPromptSubmit
  pause
  if [[ $title == *"[ask]"* ]] && [ ! -e ".natamux-task/.asked" ]; then
    mkdir -p .natamux-task && : >.natamux-task/.asked
    local question="Should empty values be left out or written as blanks?"
    printf '\e[33m?\e[0m %s\n' "$question"
    hook Notification ",\"notification_type\":\"permission_prompt\",\"message\":$(json "$question")"
    local answer
    answer=$(read_prompt)
    hook UserPromptSubmit
    say "Got it: $answer"
  fi
  if [[ $text == *"The checks of this stage failed"* ]]; then
    fix_checks "$text"
  else
    local first=yes
    [ -e ".natamux-task/.tried-${NATAMUX_TERMINAL:-1}-$(printf '%s' "$text" | cksum | cut -d' ' -f1)" ] && first=no
    mkdir -p .natamux-task && : >".natamux-task/.tried-${NATAMUX_TERMINAL:-1}-$(printf '%s' "$text" | cksum | cut -d' ' -f1)"
    local skip=no
    [[ $title == *"[miss-section]"* ]] && [ "$first" = yes ] && skip=yes
    # Code stages say whether to commit; the files they name are input, not output.
    local code=no
    [[ $text == *"Write the tests first"* || $text == *"Commit your work"* || $text == *"Do not commit"* ]] && code=yes
    if [[ $text =~ Write\ (\.natamux-task/[a-z.-]+\.md)\ with\ these\ sections:\ ([^.]+)\. ]]; then
      write_file "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "$skip"
    elif [[ $text =~ Write\ (\.natamux-task/[a-z.-]+\.md)\ with\ a\ Cases\ section ]]; then
      write_file "${BASH_REMATCH[1]}" "Cases" no
    elif [[ $text =~ (\.natamux-task/[a-z.-]+\.md) ]] && [ "$code" = no ]; then
      write_file "${BASH_REMATCH[1]}" "Summary" no
    fi
    if [ "$code" = yes ]; then
      local commit=no
      [[ $text == *"Commit your work"* ]] && commit=yes
      [[ $title == *"[no-commit]"* ]] && [ "$first" = yes ] && commit=no
      change_code "$commit"
    fi
  fi
  say "Done with this stage."
  if [[ $title != *"[no-done]"* ]]; then
    tool Bash command "natamux done"
    natamux done >/dev/null 2>&1 || true
  fi
  hook Stop
}

if [[ $prompt == *"natamux done"* ]]; then
  printf '\e[2m╭──────────────────────────────────────────────╮\e[0m\n'
  printf '\e[2m│\e[0m \e[1mFake agent\e[0m, a stand-in for Claude Code in dev \e[2m│\e[0m\n'
  printf '\e[2m╰──────────────────────────────────────────────╯\e[0m\n\n'
  # The terminal closed: stop, as an agent would.
  trap 'exit 0' HUP TERM
  next=$prompt
  while true; do
    do_stage "$next"
    printf '\e[2m(waiting for the next prompt)\e[0m\n'
    next=$(read_prompt) || exit 0
    [ -n "$next" ] || exit 0
  done
fi

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
  natamux done >/dev/null 2>&1 || true
  hook Stop
  ;;
fail)
  work 1
  tool Bash command "pnpm --filter @natamux/daemon test"
  printf '  \e[31m✗ 2 failed: git.test.ts › retries a push after a network error\e[0m\n\n'
  say "The tests fail and I cannot fix them without more context. Stopping."
  hook StopFailure
  exit 1
  ;;
*)
  work 1
  work 2
  say "Done. The change is ready for review."
  natamux done >/dev/null 2>&1 || true
  hook Stop
  ;;
esac
