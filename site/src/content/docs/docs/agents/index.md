---
title: Agents
description: Which coding agents Natamux starts, what each one reports to the dashboard, and how well each was tested.
sidebar:
  order: 0
---

Natamux starts five coding agents with hooks, and runs any other terminal agent without them. A task has one agent. A stage can start another, with its own model. See [Mix agents in one task](/docs/guides/mix-agents/).

## Compatibility

| Agent | Program | How it reports | Failed state | Skills by name | Tested |
|---|---|---|---|---|---|
| [Claude Code](/docs/agents/claude-code/) | `claude` | HTTP hooks, by `--settings` | Yes | `/name` | Real sessions |
| [Codex](/docs/agents/codex/) | `codex` | `-c hooks.<Event>=…` overrides | No | `$name` | Not run against a real session |
| [Gemini CLI](/docs/agents/gemini/) | `gemini` | A settings file, by environment variable | No | Path of a `SKILL.md` | Not run against a real session |
| [Copilot CLI](/docs/agents/copilot/) | `copilot` | `.github/hooks/natamux.json` in the worktree | Yes | `/name` | A real session, 1.0.94 |
| [Cursor CLI](/docs/agents/cursor/) | `cursor-agent` | `.cursor/hooks.json` in the worktree | Yes | Path of a `SKILL.md` | Not run against a real session |
| [Other agents](/docs/agents/other-agents/) | any | The terminal bell, `natamux ask` and `natamux done` | No | Path of a `SKILL.md` | Not applicable |

- Every agent that has hooks reports when it works, asks and finishes. The hooks are for that session only. Your own settings do not change.
- The Cursor CLI has no hook for a wait on permission, so the terminal bell marks that wait.
- "Not run against a real session" means the hooks follow the vendor's documentation and are tested with made-up input.
- Each page has the exact command line, the events and the known limits.

## Which agent for which stage

Natamux does not rank agents. Pick by what the stage needs:

| Stage | What helps |
|---|---|
| Implement | The agent you trust to change code. It runs in Shell 1. |
| Review, Security review | A different agent than the one that wrote the code, so the review has fresh eyes. These stages run in their own shell. |
| Test spec | An agent that does not read the code. It runs beside Implement. |
| A CI stage of your own | Any agent that can read a failed log. |

Set `agent` and `args` on the stage in [`.natamux.json`](/docs/reference/configuration/#stage).
