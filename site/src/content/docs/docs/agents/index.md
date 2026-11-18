---
title: Agents
description: Which coding agents Natamux starts, what each one reports to the dashboard, and how well each was tested.
sidebar:
  order: 0
---

Natamux starts 16 coding agents. Eight have hooks and report each state. The other eight, and any other terminal agent, are followed by their terminal. A task has one agent. A stage can start another, with its own model. See [Mix agents in one task](/docs/guides/mix-agents/).

## Compatibility

| Agent | Program | How it reports | Failed state | Skills by name | Tested |
|---|---|---|---|---|---|
| <img class="agent-icon" src="/agents/claude.svg" width="16" height="16" alt="" />[Claude Code](/docs/agents/claude-code/) | `claude` | HTTP hooks, by `--settings` | Yes | `/name` | Real sessions |
| <img class="agent-icon" src="/agents/codex.svg" width="16" height="16" alt="" />[Codex](/docs/agents/codex/) | `codex` | `-c hooks.<Event>=…` overrides | No | `$name` | Not run against a real session |
| <img class="agent-icon" src="/agents/gemini-ai.svg" width="16" height="16" alt="" />[Gemini CLI](/docs/agents/gemini/) | `gemini` | A settings file, by environment variable | No | Path of a `SKILL.md` | Not run against a real session |
| <img class="agent-icon agent-icon-dark" src="/agents/copilot.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/copilot_light.svg" width="16" height="16" alt="" />[Copilot CLI](/docs/agents/copilot/) | `copilot` | `.github/hooks/natamux.json` in the worktree | Yes | `/name` | A real session, 1.0.94 |
| <img class="agent-icon agent-icon-dark" src="/agents/cursor.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/cursor_light.svg" width="16" height="16" alt="" />[Cursor CLI](/docs/agents/cursor/) | `cursor-agent` | `.cursor/hooks.json` in the worktree | Yes | Path of a `SKILL.md` | Not run against a real session |
| <img class="agent-icon agent-icon-dark" src="/agents/opencode.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/opencode_light.svg" width="16" height="16" alt="" />[OpenCode](/docs/agents/opencode/) | `opencode` | A plugin file, `.opencode/plugins/natamux.js` in the worktree | Yes | Path of a `SKILL.md` | Not run against a real session |
| <img class="agent-icon" src="/agents/qwen.svg" width="16" height="16" alt="" />[Qwen Code](/docs/agents/qwen/) | `qwen` | `.qwen/settings.json` in the worktree | Yes | Path of a `SKILL.md` | Not run against a real session |
| <img class="agent-icon agent-icon-dark" src="/agents/droid.svg" width="16" height="16" alt="" /><img class="agent-icon agent-icon-light" src="/agents/droid_light.svg" width="16" height="16" alt="" />[Factory Droid](/docs/agents/droid/) | `droid` | `.factory/hooks.json` in the worktree | No | Path of a `SKILL.md` | Not run against a real session |
| <img class="agent-icon" src="/agents/amp.svg" width="16" height="16" alt="" />[More terminal agents](/docs/agents/more-agents/) | `amp`, `kimi`, `goose`, `aider`, `crush`, `cline`, `kiro-cli`, `muse` | The terminal bell, `natamux ask` and `natamux done` | No | Path of a `SKILL.md` | Not run against a real session |
| [Other agents](/docs/agents/other-agents/) | any | The terminal bell, `natamux ask` and `natamux done` | No | Path of a `SKILL.md` | Not applicable |

- Every agent that has hooks reports when it works, asks and finishes. The hooks are for that session only. Your own settings do not change.
- The Cursor CLI has no hook for a wait on permission, so the terminal bell marks that wait. The bell also counts for Copilot CLI, Qwen Code (its hooks run only after you approve them) and Droid.
- Amp, Kimi, Goose, Aider, Crush, Cline, Kiro and Muse Code have no hooks in Natamux. Their first prompt is pasted into the terminal. See [More terminal agents](/docs/agents/more-agents/).
- The new agents (OpenCode, Qwen Code, Droid and the eight terminal agents) show no token numbers.
- "Not run against a real session" means the hooks follow the vendor's documentation and are tested with made-up input.
- Each page has the exact command line, the events and the known limits.

## Which agent for which stage

Natamux does not rank agents. Pick by what the stage needs:

| Stage | What helps |
|---|---|
| Implement | The agent you trust to change code. It runs in Terminal 1. |
| Review, Security review | A different agent than the one that wrote the code, so the review has fresh eyes. These stages run in their own terminal. |
| Test spec | An agent that does not read the code. It runs beside Implement. |
| A CI stage of your own | Any agent that can read a failed log. |

Set `agent` and `args` on the stage in [`.natamux.json`](/docs/reference/configuration/#stage).
