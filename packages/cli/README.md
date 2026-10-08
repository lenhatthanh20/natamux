# @helmsman/cli

The `helmsman` command. It talks to the daemon over HTTP. Agents can call it from any shell.

Run in a codespace after `pnpm build`: `pnpm --filter @helmsman/cli start <command>`.

## Settings (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `HELMSMAN_TOKEN` | none, required | Token the daemon printed when it started |
| `HELMSMAN_URL` | `http://127.0.0.1:4317` | Address of the daemon |

## Commands

```sh
helmsman task create "Fix the bug" --priority high --label bug --depends-on T-0007
helmsman task list --state review
helmsman task show T-0001
helmsman event T-0001 task.readied
helmsman event T-0001 agent.asked --source agent --payload '{"question":"Which branch?"}'
helmsman events T-0001
helmsman workspace create T-0001
helmsman workspace remove T-0001 --yes --delete-branch
```

```sh
export HELMSMAN_TOKEN=<token> HELMSMAN_TASK_ID=T-0001
claude --settings "$(helmsman hooks claude-code)"
```

`helmsman hook <agent>` is for an agent that can only run a command as its hook: it reads the hook's JSON on stdin and sends it to `POST /hooks/<agent>` of the daemon. It does nothing unless `HELMSMAN_TOKEN` is set (so it is harmless in a session that is not Helmsman's), always exits 0 and prints nothing, so it can never get in the agent's way. Set `HELMSMAN_DEBUG=1` to see why a hook did not arrive. It does not use the proxy variables.

`hooks claude-code` prints Claude Code settings that report the session's progress to the daemon, so the task moves between running, needs input and in review by itself. It needs no token to print and never writes the token into the output. See `docs/claude-code-adapter.md`.

`workspace create` makes a git worktree and a branch for the task. `workspace remove` deletes the worktree folder, so it needs `--yes`. Git refuses when the worktree has files that are not committed. With `--delete-branch` the branch goes too, but only when it is fully merged.

Add `--json` to any command to print JSON. `helmsman help` lists every option and event type.

Exit codes: `0` done, `1` the request failed (daemon not reachable, wrong token, task not found), `2` the command line is wrong.

An event that does not fit the task's state prints `Not accepted` and exits `0`: the daemon recorded it and left the task as it was.
