---
title: More terminal agents
description: Amp, Kimi, Goose, Aider, Crush, Cline, Kiro and Muse Code in a Natamux task, followed by their terminal.
sidebar:
  order: 9
---

Natamux starts eight more agents. None of them has hooks in Natamux. The dashboard follows each by its terminal: output, the terminal bell, `natamux ask` and `natamux done`. See [What Natamux sees without hooks](/docs/agents/other-agents/#what-natamux-sees-without-hooks).

## Commands

| Agent | Program | Command line | Auto flag | Model | Resume | Sign in |
|---|---|---|---|---|---|---|
| <img class="agent-icon" src="/agents/amp.svg" width="16" height="16" alt="" />Amp | `amp` | `amp --dangerously-allow-all [extra args]` | `--dangerously-allow-all` | No field | None | `amp login` |
| <img class="agent-icon agent-icon-dark" src="/agents/kimi.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/kimi_light.svg" width="16" height="16" alt="" />Kimi | `kimi` (or `kimi-code`) | `kimi --yolo [extra args]` | `--yolo` | `--model` | `--session <id>`, `--continue` | `kimi login` |
| <img class="agent-icon agent-icon-dark" src="/agents/goose.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/goose_light.svg" width="16" height="16" alt="" />Goose | `goose` | `goose session [extra args]` | `GOOSE_MODE=auto` before the program | No field (set `GOOSE_MODEL`) | `--resume --session-id <id>`, `--resume` | Not found in its docs |
| <img class="agent-icon agent-icon-dark" src="/agents/aider.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/aider_light.svg" width="16" height="16" alt="" />Aider | `aider` | `aider --yes-always [extra args]` | `--yes-always` | `--model` | `--restore-chat-history` (newest only) | Not found in its docs |
| <img class="agent-icon" src="/agents/crush.svg" width="16" height="16" alt="" />Crush | `crush` | `crush --yolo [extra args]` | `--yolo` | No field | `--session <id>`, `--continue` | Not found in its docs |
| <img class="agent-icon agent-icon-dark" src="/agents/cline.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/cline_light.svg" width="16" height="16" alt="" />Cline | `cline` | `cline --auto-approve true [extra args]` | `--auto-approve true` | `--model` | `--id <session>` | `cline auth` |
| <img class="agent-icon" src="/agents/kiro.svg" width="16" height="16" alt="" />Kiro | `kiro-cli` | `kiro-cli chat --trust-all-tools [extra args]` | `--trust-all-tools` | `--model` | `--resume-id <id>`, `--resume` | A browser sign-in |
| <img class="agent-icon" src="/agents/muse.svg" width="16" height="16" alt="" />Muse Code | `muse` | `muse --trust-workspace --yolo [extra args]` | `--yolo` | `--model` | `muse resume <id>` | A browser sign-in, or `META_API_KEY` |

- The auto flag is left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents).
- The Model field of **Start a task** goes to the agent as `--model '<name>'`. The app hides the field for Amp, Goose and Crush: they have no such flag in their interactive mode.
- Change a program with `NATAMUX_<ID>_COMMAND` and add arguments with `NATAMUX_<ID>_ARGS`, such as `NATAMUX_KIRO_COMMAND`. See the [variables](/docs/reference/environment-variables/).
- Start one from the dashboard, or `natamux agent start T-42 --agent goose`.
- Kimi has two products with the name: the Python Kimi Code CLI and a newer Kimi Code 2. Natamux accepts both programs and passes `--yolo`.
- The sign-in column is from each vendor's docs. Natamux does not know a sign-in screen of these agents, so a task that waits for a login is not marked "sign in". Sign in once in a normal terminal first.

## The first prompt is pasted

None of these agents takes a first prompt on its command line and stays open. So Natamux starts the agent with no prompt, and pastes the prompt into its terminal when the agent's first screen has settled. That is when the agent runs in front of the shell, it printed, and then 1.5 seconds went by with no output. If the agent printed nothing, it pastes after 30 seconds. Each prompt is pasted once. If the agent ended, or did not start within 30 seconds, the prompt is dropped: it never goes to the shell, which would run it as a command.

## What the dashboard sees

- The task runs when the agent prints, and shows as quiet after 10 minutes of silence.
- The terminal bell moves a running task to needs input.
- The agent ends a stage with `natamux done` and asks you with `natamux ask`. Tell it about both commands in its instructions.
- There is no session from hooks. A task comes back on the session only for an agent with a resume flag above.
- No token numbers for these agents.

## Commit message drafts

A short answer, such as a commit message draft, must not change files. Goose answers in its `chat` mode, which runs no tools, and Kiro in `kiro-cli chat --no-interactive`. Amp, Kimi, Aider, Crush, Cline and Muse Code have no mode that only answers: their one run is a whole turn that can change files. So their drafts come from Claude Code.

## Muse Code hooks

Meta's docs name a hook file for a project, `.muse/hooks.json`. Muse runs hooks with a cleared environment, so `natamux hook` would not see `NATAMUX_TOKEN`. Natamux does not write the file yet.

## Tested

None of these was run against a real session. The commands follow each vendor's docs or source, checked on 2026-11-17, and are tested with made-up input.

## Skills

| Agent | Project folders | You |
|---|---|---|
| Amp | `.agents/skills`, `.claude/skills` | `~/.config/agents/skills`, `~/.agents/skills`, `~/.config/amp/skills`, `~/.claude/skills` |
| Kimi | `.kimi/skills`, `.claude/skills`, `.codex/skills`, `.agents/skills` | `~/.kimi/skills`, `~/.claude/skills`, `~/.config/agents/skills`, `~/.agents/skills` |
| Crush | `.agents/skills`, `.crush/skills`, `.claude/skills`, `.cursor/skills` | `~/.config/crush/skills`, `~/.agents/skills`, `~/.claude/skills` |
| Cline | `.cline/skills` | `~/.cline/skills` |
| Kiro | `.kiro/skills` | `~/.kiro/skills` |
| Muse Code | `.agents/skills`, `.codex/skills`, `.claude/skills` | `~/.config/muse/skills`, `~/.agents/skills`, `~/.claude/skills` |

Goose and Aider have no skill folder that Natamux knows. For `{skill:<name>}`, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt).
