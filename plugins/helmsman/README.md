# Helmsman plugin for Claude Code

A skill that teaches Claude Code to work inside a Helmsman task with the `helmsman` CLI: read the task, work stage by stage, ask the user, report each stage done with `helmsman done`, file follow-up tasks.

Install, in Claude Code:

```
/plugin marketplace add lenhatthanh20/helmsman
/plugin install helmsman@helmsman
```

The skill only matters in a session that Helmsman started (it sets `HELMSMAN_TASK_ID`). The CLI must be on the `PATH`, with `HELMSMAN_TOKEN` and `HELMSMAN_URL` set; Helmsman's terminals set them.
