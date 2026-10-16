---
title: Mix agents in one task
description: Give each stage of a task its own agent and model, and see which shell each one runs in.
sidebar:
  order: 5.2
---

One task can use several agents. Claude Code writes the code, Cursor reviews it, Copilot watches CI. Each stage picks its agent and model in `.natamux.json`.

## Why

- Each agent is good at different jobs. Pick the one that fits the stage.
- A new agent reviews with fresh eyes. It did not write the code, so it does not defend it.
- Several providers share the cost and the rate limits.

## How

Set `agent` and `args` on a stage. In **Project settings → Stages**, they are **Agent** and **Arguments**.

```json
{ "id": "agent-review", "kind": "agent", "gate": "auto", "output": "review.md",
  "ownShell": true, "agent": "cursor", "args": "--model gpt-5" }
```

- `agent` is `claude-code`, `codex`, `gemini`, `copilot` or `cursor`.
- `args` adds words to the command line of that agent, such as a model flag. See [Choose a model](/docs/concepts/stages/#choose-a-model).
- A catalog stage in Shell 1, such as Spec or Implement, cannot change them: that would end the session of the stages before it.
- A stage of your own (made with **Custom stage**) can. It then runs in a shell of its own, from Shell 6, as Follow CI does in the Team example.
- The [configuration reference](/docs/reference/configuration/) lists where `agent` and `args` are allowed.

## Shells

Each task has a few shells. See [Shells and agents](/docs/concepts/stages/#shells-and-agents) for the full table.

| Shell | What runs there |
|---|---|
| Shell 1 | The task's agent, in one session for the stages in a row. It is never closed before the task ends. |
| Shell 2 | **Test spec**, when it has no agent of its own. |
| Shell 5 | A stage with `ownShell` and no agent or `args` of its own. |
| Shell 6 and up | A stage with its own `agent` or `args`. The first such stage of the pipeline gets Shell 6, the next Shell 7. |

A stage that runs again, for example after a send back, gets its prompt in the agent of its shell. That agent still knows the last round. To start fresh each time, set `"endShell": true`.

### End a stage's shell

`"endShell": true` closes the shell when the stage is done. This frees memory, and the next run starts a new session. It never closes Shell 1. See [End a stage's shell](/docs/concepts/stages/#end-a-stages-shell).

## Run with a workflow

`"workflow": true` offers **Run with a workflow** on a stage. Claude Code then works with sub-agents inside that one stage. Use it on a hard stage such as Implement. It uses more tokens, and only Claude Code does it. See [Run a stage as a workflow](/docs/concepts/stages/#run-a-stage-as-a-workflow).

## Example

Stages of the two examples in [Build a workflow](/docs/guides/workflows/):

| Stage | Example | Agent | Shell | Why |
|---|---|---|---|---|
| Spec, Implement | Solo | Claude Code, the task's agent | Shell 1 | One session keeps the context of the spec for the code. Implement adds sub-agents with a workflow. |
| Test spec | Solo | Copilot | Its own, closed when done | Writes cases without reading the code. The shell frees its memory. |
| Review | Solo | Cursor | Its own, closed when done | An agent that did not write the code reads it. |
| Watch CI and merge | Solo | Copilot | Its own | Moves CI and merge work off the main session. |
| Spec, Plan, Implement | Team | Task's agent | Shell 1 | One session keeps the context of the spec for the code. |
| Review PR | Team | Claude Code | Its own, closed when done | Reads a teammate's work with a new session. |
| Review implementation | Team | Claude Code | Its own, closed when done | A new session per round. |
| Follow CI | Team | Cursor | Its own, closed when done | Watches jobs, and a failed job sends the task back. |
| Verify impact list | Team | Claude Code | Its own, closed when done | Checks the change with a new session. |
| Definition of done and review request | Team | Copilot | Its own, closed when done | Another provider for the last check. |

## What each agent needs

Every agent you use in a stage must run on the machine of the task.

- Installed, and its command runs in a terminal.
- Logged in.
- Hooks: Natamux adds them for each agent it starts, so the dashboard shows its state.

Details per agent:

- [Claude Code](/docs/agents/claude-code/)
- [Codex](/docs/agents/codex/)
- [Gemini CLI](/docs/agents/gemini/)
- [Copilot CLI](/docs/agents/copilot/)
- [Cursor CLI](/docs/agents/cursor/)
- [Other agents](/docs/agents/other-agents/): without hooks, Natamux sees less

## The agent token

Task terminals get an agent token, not the daemon's token. An agent cannot approve, skip or pass a stage, so a stage with another provider cannot move the task past a gate. It can run `natamux done` and `natamux stage back`. See [Security and privacy](/docs/help/security-and-privacy/#the-daemon) and [The agent token](/docs/reference/http-api/#the-agent-token).

## Related

- [Build a workflow](/docs/guides/workflows/): the two example files
- [Stages](/docs/concepts/stages/): shells, models and workflows
- [Agents and hooks](/docs/concepts/agents-and-hooks/): how each agent reports
