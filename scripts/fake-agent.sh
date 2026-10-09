#!/usr/bin/env bash
# A stand-in for Claude Code in the development setup, where claude is not installed.
# scripts/dev.sh seed starts the daemon with it. It prints steps, edits a notes file in the
# workspace, and reports to Helmsman through the same hooks that Claude Code calls.
#
# A stage prompt (it ends with "run `helmsman done`") is done for real, roughly: the files the
# prompt names are written with the sections it names, code stages commit, and failed checks that
# come back are fixed. Then it waits for the next prompt, as Claude Code does. Marks in the task
# title make it go wrong on purpose, for tests:
#   [miss-section]  leaves out the last section the first time
#   [no-commit]     changes code but does not commit, the first time
#   [no-done]       ends its turn without `helmsman done`
#   [ask]           asks a question and waits for the answer first
#
# Any other prompt picks what it does: it starts with "loop:", "ask:", "fail:" or nothing.
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

# The next prompt typed or pasted into the terminal: the first line, then the lines that follow
# at once. A paste from the dashboard comes in bracket marks, which are dropped.
read_prompt() {
  local line text
  IFS= read -r line || exit 0
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
    *"has no case under Cases"*) file=".helmsman-task/${line#- }" && file=${file%% has*} &&
      printf '## Cases\n\n### One more case\n\n' >>"$file" ;;
    *" has no "*" section"*)
      file=".helmsman-task/${line#- }" && file=${file%% has no*}
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
  if [[ $title == *"[ask]"* ]] && [ ! -e ".helmsman-task/.asked" ]; then
    mkdir -p .helmsman-task && : >.helmsman-task/.asked
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
    [ -e ".helmsman-task/.tried-${HELMSMAN_TERMINAL:-1}-$(printf '%s' "$text" | cksum | cut -d' ' -f1)" ] && first=no
    mkdir -p .helmsman-task && : >".helmsman-task/.tried-${HELMSMAN_TERMINAL:-1}-$(printf '%s' "$text" | cksum | cut -d' ' -f1)"
    local skip=no
    [[ $title == *"[miss-section]"* ]] && [ "$first" = yes ] && skip=yes
    if [[ $text =~ Write\ (\.helmsman-task/[a-z.-]+\.md)\ with\ these\ sections:\ ([^.]+)\. ]]; then
      write_file "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "$skip"
    elif [[ $text =~ Write\ (\.helmsman-task/[a-z.-]+\.md)\ with\ a\ Cases\ section ]]; then
      write_file "${BASH_REMATCH[1]}" "Cases" no
    elif [[ $text =~ (\.helmsman-task/[a-z.-]+\.md) ]] && [[ $text != *"Write the tests first"* ]] && [[ $text != *"Commit your work"* ]]; then
      write_file "${BASH_REMATCH[1]}" "Summary" no
    fi
    if [[ $text == *"Write the tests first"* ]] || [[ $text == *"Commit your work"* ]]; then
      local commit=yes
      [[ $title == *"[no-commit]"* ]] && [ "$first" = yes ] && commit=no
      change_code "$commit"
    fi
  fi
  say "Done with this stage."
  if [[ $title != *"[no-done]"* ]]; then
    tool Bash command "helmsman done"
    helmsman done >/dev/null 2>&1 || true
  fi
  hook Stop
}

if [[ $prompt == *"helmsman done"* ]]; then
  printf '\e[2m╭──────────────────────────────────────────────╮\e[0m\n'
  printf '\e[2m│\e[0m \e[1mFake agent\e[0m, a stand-in for Claude Code in dev \e[2m│\e[0m\n'
  printf '\e[2m╰──────────────────────────────────────────────╯\e[0m\n\n'
  next=$prompt
  while true; do
    do_stage "$next"
    printf '\e[2m(waiting for the next prompt)\e[0m\n'
    next=$(read_prompt)
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
