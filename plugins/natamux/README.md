# Natamux plugin for Claude Code

A skill that teaches Claude Code to work inside a Natamux task with the `natamux` CLI: read the task, work stage by stage, ask the user, report each stage done with `natamux done`, file follow-up tasks.

Install, in Claude Code:

```
/plugin marketplace add lenhatthanh20/natamux
/plugin install natamux@natamux
```

The skill only matters in a session that Natamux started (it sets `NATAMUX_TASK_ID`). The CLI must be on the `PATH`, with `NATAMUX_TOKEN` and `NATAMUX_URL` set; Natamux's terminals set them.
