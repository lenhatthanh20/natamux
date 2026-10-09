# Design: task stages, task context and issue links

Status: draft for review. Nothing here is built yet.

## 1. Goal

A task moves through the steps of real work, not only "the agent runs, then I review":

```
                        ┌─ Test spec ─┐
Context → Spec → Plan → ┤             ├→ Review → Testing → Release → (done)
                        └─ Implement ─┘
```

- This is the default for every project. A project can change the list: drop, reorder, add, or run stages side by side (section 6).
- Test spec and Implement run at the same time. Review starts when both are finished (section 3.6).
- Each stage makes something you can read: `context.md`, `spec.md`, `plan.md`, `test-spec.md`, a diff, test results, a pull request.
- Some stages wait for your approval (a gate). Others move on by themselves.
- Context can come from several places: typed by hand, a GitHub issue, or an agent that uses a skill or an MCP server (Jira, Linear, Notion, …).
- A task made in the app can open a GitHub issue for itself. The pull request links to the issue.

This builds the v1 feature "Approval gates" in `plan-features.md` 5.2, and extends "Import tasks from GitHub issues".

## 2. Two axes: stage and state

A task has two fields that answer two questions:

| Field | Question | Values |
|---|---|---|
| `stage` | Where in the work is it? | The stage ids of its project, by default `context`, `spec`, `plan`, `test-spec`, `implement`, `review`, `testing`, `release` |
| `state` | What is happening in this stage now? | The existing states: `draft`, `ready`, `running`, `needs-input`, `review`, `blocked`, `failed`, `done`, `cancelled` |

The state list does not change. Each state now means "in the current stage":

| State | Meaning now | Shown as |
|---|---|---|
| `draft` | The stage cannot start yet; something is missing (for example no context source) | Not started |
| `ready` | The stage can start | Ready |
| `running` | An agent or a command works on the stage | Running |
| `needs-input` | The agent asks a question | Needs input |
| `review` | The stage output waits for your approval | Waiting for approval |
| `blocked`, `failed` | As before | Blocked, Failed |
| `done` | The last stage is approved. Only reachable from the last stage | Done |
| `cancelled` | As before | Cancelled |

"Done" is not a stage. It is the state after the last stage.

Why keep the state list: the "Needs me" strip, the notifications, the agent hooks and the CLI all work on states and keep working. A task in `review` at stage `plan` shows in "Needs me" as "Plan waits for approval".

In a parallel step each stage has its own state, and `state` is the most urgent of them (section 3.6).

## 3. Stages

### 3.1 Stage kinds

Every stage has a kind. The kind decides what runs and when the stage ends.

| Kind | What runs | Output | Ends when |
|---|---|---|---|
| `context` | The context sources of the task (section 4) | `context.md` | Sources are read; then the gate |
| `agent` | The task's agent with the stage prompt | A file named by the stage (`spec.md`, `plan.md`), or code changes | The agent finishes its turn (`agent.finished`); then the gate |
| `review` | Nothing. You read the diff, comment, approve or send back | Review comments | You approve, or send it back to an earlier stage |
| `testing` | The agent with the stage prompt when there is one, then a command (for example `pnpm test`) in a task terminal | New tests, the command output | The command exits: 0 passes, other fails |
| `release` | Push and open a pull request (one click, or on entry when the gate is `auto`) | The pull request | The pull request is merged |
| `manual` | Nothing. A step you do yourself | — | You approve |

### 3.2 The default pipeline

| Stage | Kind | Gate | Prompt or command (default) |
|---|---|---|---|
| `context` | context | manual | — |
| `spec` | agent | manual | Read `context.md`. Write `spec.md`: goal, scope, out of scope, acceptance criteria. Do not change code |
| `plan` | agent | manual | Read `context.md` and `spec.md`. Write `plan.md`: the steps, the files to change, the tests to add. Do not change code |
| `test-spec` (parallel) | agent | manual | Read `context.md` and `spec.md`, not the code. Write `test-spec.md`: test cases with input, expected result and edge cases. Do not change code |
| `implement` (parallel) | agent | auto | Read `spec.md` and `plan.md`. Write the tests first, then the code that makes them pass. Commit when done |
| `review` | review | manual | — |
| `testing` | testing | auto | An agent turns each case in `test-spec.md` into a test that is not there yet, then the project's test command runs all tests |
| `release` | release | manual | — |

- **Gate `manual`:** when the stage output is ready, the state becomes `review` and the task waits for you.
- **Gate `auto`:** when the stage output is ready, the task moves to the next stage at once.
- **Gate `auto` and failures:** a failed `testing` stage goes to `failed`. You then retry it, or send the task back to `implement` with the test output as the next prompt.

### 3.3 Moving between stages

Four actions, each one an event:

| Action | Event | Who | Effect |
|---|---|---|---|
| Approve | `stage.approved` | human | `review` → the next stage, state `ready`. The last stage → state `done` |
| Pass | `stage.passed` | system | An `auto` gate: the next stage, state `ready` |
| Send back | `stage.reopened` (`payload.to`) | human | An earlier stage, state `ready`. The review comment becomes the next prompt |
| Skip | `stage.skipped` | human | The next stage, without output. For small tasks that need no spec or plan |

- **Entering a stage with an agent:** the daemon starts the stage at once.
  - When the agent runs already, the stage prompt is sent to it as the next prompt, as review comments are today, and the agent keeps what it learned in earlier stages.
  - When no agent runs, the daemon starts one with the stage prompt.
- **`review.approved`:** stays for old clients and means the same as `stage.approved`.
- **`state.overridden`:** still sets any state, and now also takes an optional `stage`.

### 3.4 Where stage files live

Stage files are `context.md`, `spec.md`, `plan.md`, `test-spec.md` and test output.

- **While the task has a workspace:** they are in `.helmsman-task/` inside the worktree.
  - The agent reads and writes there without leaving its folder, so no extra permission prompts.
  - The daemon adds `.helmsman-task/` to the repository's `.git/info/exclude`, so the files are never committed.
- **Without a workspace, and as a copy:** they are in `<data dir>/tasks/T-0001/`.
  - The daemon copies the worktree files there on each approval and before the workspace is removed.
  - A removed workspace does not lose them.
- **Editing:** you can edit a stage file in the app before you approve it, for example to fix the spec by hand.

### 3.5 Checks

When an `agent` stage ends without its output file, for example the agent stopped to ask a question in plain text, the state becomes `needs-input`, not `review`. The task header says "spec.md is missing".

### 3.6 Parallel stages

A step of the pipeline is one stage, or a group of stages that run at the same time. The default has one group: Test spec and Implement.

- **Lanes:** while a task is in a group, `stage` is the group id (`build`) and a new field `lanes` holds the state of each stage in it, for example `{ "test-spec": "review", "implement": "running" }`. Outside a group `lanes` is empty.
- **Gates per lane:** each stage keeps its own gate. Test spec waits for your approval; Implement passes by itself.
- **End of the group:** when every lane is finished (approved or passed), the task moves to the next step. Review then shows the diff and `test-spec.md` together.
- **Task state:** `state` is the most urgent lane state, in this order: `needs-input`, `failed`, `blocked`, `review`, `running`, `ready`. So "Needs me" shows the task when any lane needs you.
- **Agents:** each lane has its own agent session in its own terminal, in the same worktree. Implement keeps terminal 1 and the session of the earlier stages; Test spec gets a new terminal.
  - The terminal gets `HELMSMAN_STAGE`, and `helmsman hook` sends it with each hook, so the daemon knows which lane a hook is about.
  - Test spec writes only `.helmsman-task/test-spec.md`, which git ignores, so the two agents do not touch the same files.
- **Why Test spec does not read the code:** it is written from the spec alone, like a separate tester, so it checks what was asked, not what was built. Implement does not wait for it; it writes its own tests from the spec and the plan. In Testing, an agent adds a test for each case in `test-spec.md` that the code does not test yet.
- **Events name the lane:** `stage.approved`, `stage.passed`, `stage.reopened` and `stage.skipped` carry `payload.stage` inside a group.
- **Send back:** Review can send the task back to one stage of a group. That lane runs again; the other keeps its result.
- **Rules:** groups do not nest, hold 2 to 4 stages, and hold only `agent` and `manual` kinds.

## 4. Task context

### 4.1 Sources

| Type | Who reads it | Input when the task is made | Built in this work |
|---|---|---|---|
| `manual` | Nobody; you type it | Text, links | Yes |
| `github-issue` | The daemon, with `gh issue view --comments` | Issue number or address | Yes |
| `skill` | The task's agent, with prompt `/<skill> <ref>` | The ticket key or address | Yes |
| `mcp` | The task's agent, told which MCP server to use | The ticket key or address | Yes |
| `url` | The daemon fetches the page | An address | Later |

Agent sources (`skill`, `mcp`):
- The agent runs in the task's worktree, with the prompt from the project settings.
- It writes `context.md`.
- Helmsman does not set up MCP servers or skills. The agent uses the ones it already has (its user settings or the repository's `.mcp.json`).

The `context` stage ends when `context.md` exists. With gate `manual` you read it, edit it if needed, and approve.

### 4.2 Task fields

```ts
context: { source: 'manual' | 'github-issue' | 'skill' | 'mcp'; ref: string } | null
issue: { provider: 'github' | string; key: string; url: string } | null
```

- `context` says where the context came from.
- `issue` is the link to the tracker item: `#42` on GitHub, `ABC-123` on Jira.
- A task imported from an issue gets both. Import now finds earlier imports by `issue.url`, not by the "Imported from" line in the notes. The old line is still read, for tasks made before this change.

## 5. Issue links (tracker)

### 5.1 When an issue is made

`tracker.createIssue` in the project settings:

| Value | When |
|---|---|
| `never` | Never |
| `manual` | You click "Create issue" on the task |
| `on-spec-approved` (default) | When you approve the `spec` stage. The issue body is `spec.md` |
| `on-pr` | When the pull request is opened, just before it |

- When the pipeline has no `spec` stage, `on-spec-approved` acts as `on-pr`.
- A task that has `issue` already never makes another one. "Create issue" becomes "Open issue".

### 5.2 Providers

- **`github`:** the daemon runs `gh issue create --title --body` in the repository and keeps the address.
- **Other trackers (Jira, Linear):** later.
  - The plan: an agent with that tracker's MCP server makes the item, then calls `helmsman task link T-0001 --issue ABC-123 --url …`.
  - The CLI command is part of this work, so an agent can already link an item by hand.

### 5.3 Pull request

When `tracker.linkInPr` is on (the default):
- The pull request body ends with `Closes #42` (GitHub).
- For another tracker, it ends with the key and address.

Comments on the issue when the stage changes are later.

## 6. Project settings

The app has one default pipeline: the eight stages of section 1, with Test spec and Implement side by side. A repository without settings uses it.

A project changes it in a file `.helmsman.json` at the root of the repository, next to `.helmsman-setup`. It is committed, so the team shares it. Every field is optional; a missing field takes the default.

The name is not `.helmsman/project.json`, because `.helmsman/` is the daemon's default data folder.

```json
{
  "stages": [
    { "id": "context", "kind": "context", "gate": "manual" },
    { "id": "spec", "kind": "agent", "gate": "manual", "output": "spec.md" },
    { "id": "plan", "kind": "agent", "gate": "manual", "output": "plan.md" },
    {
      "id": "build",
      "parallel": [
        { "id": "test-spec", "kind": "agent", "gate": "manual", "output": "test-spec.md" },
        { "id": "implement", "kind": "agent", "gate": "auto" }
      ]
    },
    { "id": "review", "kind": "review", "gate": "manual" },
    { "id": "testing", "kind": "testing", "gate": "auto", "command": "pnpm test" },
    { "id": "release", "kind": "release", "gate": "manual" }
  ],
  "context": {
    "sources": [
      { "type": "manual" },
      { "type": "github-issue" },
      { "type": "mcp", "name": "Jira", "prompt": "Use the jira MCP server to read {ref}, with its comments. Write a summary to {dir}/context.md" },
      { "type": "skill", "name": "Ticket", "skill": "fetch-ticket" }
    ]
  },
  "tracker": { "type": "github", "createIssue": "on-spec-approved", "linkInPr": true }
}
```

Stage fields:
- `id`, `kind`, `gate`: required in each stage entry. `id` is lower case words joined by "-".
- `name`: shown in the app, default from the id.
- `prompt`: replaces the default prompt. Placeholders: `{title}`, `{description}`, `{dir}` (the stage file folder), `{ref}` (the context reference).
- `output`: the file the stage must write (`agent` kind).
- `command`: the test command (`testing` kind).
- A group is `{ "id", "name"?, "parallel": [stages] }` (section 3.6).

Other rules:
- **Custom stages:** a project can drop, reorder or add stages, for example a `security` stage of kind `agent` with its own prompt. It can also group stages, or take a stage out of a group so it runs on its own.
- **Snapshot:** a new task copies the list of stage ids to the task (`stages` field). Later edits to the list do not move tasks that are on the way. Prompts, gates and commands are read from the current settings, so a better prompt helps tasks that are running already. A stage id that is gone from the settings acts as kind `manual`.
- **Editing:** the app has a **Project settings** page that edits this file in the repository's main folder. You commit it yourself. "Reset to default" writes the default pipeline back.
- **Wrong settings:** a file that fails to parse shows an error on the page, and the daemon uses the defaults.

## 7. Changes per package

### 7.1 `core`

- `stage.ts`:
  - The `projectSettingsSchema` (zod) and the default pipeline.
  - `nextStep(steps, id)`, and `combinedState(lanes)` for the order in 3.6.
  - `applyTaskEvent(position, event, pipeline)`, where `position` is `{ stage, state, lanes }`.
- `task.ts`: new fields `stage`, `stages`, `lanes`, `context`, `issue`.
- `events.ts`: new events `stage.approved`, `stage.passed`, `stage.reopened`, `stage.skipped`, `stage.started`, `context.fetched`, `issue.linked`, `issue.created`.
- Tests for each move in 3.3, the gates, parallel lanes, the snapshot rule, and the migration in section 8.

### 7.2 `daemon`

- `project-settings.ts`: reads `.helmsman.json` of a repository, caches it, and watches the file.
- `stage-runner.ts`:
  - Enters a stage: starts the agent or sends it the prompt, or runs the test command, or opens the pull request.
  - Enters a group: one agent per lane, a new terminal for each lane after the first.
  - Listens to `agent.finished`, finds the lane by `HELMSMAN_STAGE`, checks the output file, and passes or waits at the gate.
- `stage-files.ts`: the folder rules of 3.4 and the `.git/info/exclude` entry.
- `context-reader.ts`: the `manual` and `github-issue` sources. `skill` and `mcp` go through `stage-runner`.
- `issue-linker.ts`: `gh issue create`, and the `Closes #N` line for the pull request body (`workspace-service.openPullRequest`).
- Routes:
  - `POST /tasks/:id/stage` with `{ action: 'approve' | 'reopen' | 'skip', stage?, to? }`.
  - `GET` and `PUT /tasks/:id/files/:name`.
  - `GET` and `PUT /repos/:id/settings`.
  - `POST /tasks/:id/issue`.
- `task-index.ts`: a `stage` column, so the board can group by it.
- The testing command reports through the CLI:

  ```
  <command> && helmsman stage pass T-0001 || helmsman stage fail T-0001
  ```

### 7.3 `cli`

New commands:
- `helmsman stage approve|back|skip|pass|fail <task> [--stage <id>]`. Inside an agent terminal `--stage` comes from `HELMSMAN_STAGE`.
- `helmsman task link <task> --issue <key> --url <url>`.
- `helmsman task create --context github-issue:42`.

### 7.4 `web`

- **Task header:** a stage stepper with done, current and next stages. The current one shows its state. A group shows its stages stacked, each with its own state and its own Approve button.
- **Approve and Send back:** buttons, with the existing `a` and `r` keys.
- **Docs tab:** for each task, `context.md`, `spec.md`, `plan.md`, `test-spec.md`, readable as markdown and editable before approval.
- **Table:** a "Stage" column. The state badge reads "Plan · Waiting for approval"; in a group, "Test spec · Waiting for approval, Implement · Running".
- **Board:** columns by step, plus Done. A group is one column. A switch keeps the old "by state" columns.
- **New task and New workspace dialogs:** a context source picker (from the project settings) and a reference field (`#42`, `ABC-123`).
- **Project settings page:** the stage list (order, on or off, gate, prompt, "run alongside the stage above"), the context sources, and the tracker.
- **Issue:** a link on the task, and "Create issue" when there is none.
- **Needs me:** keeps working on states. The text names the stage.

## 8. Old tasks

Task files without `stage` are read like this. Nothing is rewritten until the task changes.

| Old state | Stage | State |
|---|---|---|
| `review` | `review` | `review` |
| `done` | the last stage | `done` |
| any other | `implement` | unchanged |

- **`stages`:** old tasks get `implement`, `review`, `release`. That is the work they had before, so no old task suddenly waits for a spec.
- **`issue`:** an old task with an "Imported from <url>" line gets `issue` from that line.

## 9. Build order

Separate pull requests, in this order:

1. `core`: stages, groups and lanes, settings schema, events, migration, tests.
2. `daemon` and `cli`: settings file, stage runner, stage files, routes, CLI commands.
3. Context sources: `manual`, `github-issue`, `skill`, `mcp`.
4. Tracker: GitHub issue on spec approval, manual "Create issue", `Closes #N` in the pull request.
5. `web`: stepper, Docs tab, stage column and board, source picker, Project settings page.
6. Docs: `plan-features.md` rows, README, CLI docs.

Step 5 needs the UI pull requests #54 to #58 merged first; it builds on their components.

## 10. Open points

- **One agent session for all stages, or a new one per stage?** The plan above keeps one session for the main line (less to set up, the agent remembers), and a new one for each extra lane of a group. Bigger tasks may need a fresh session for `implement`. That can be a later stage setting (`session: "new"`).
- **Two agents in one worktree:** safe in the default group, because Test spec writes only an ignored file. A custom group of two code-writing stages would clash; the settings page warns when a group has more than one stage without `output`.
- **Agent stops early:** the Claude Code `Stop` hook fires after each turn. An agent that asks a question in plain text "finishes" its turn. The output file check in 3.5 covers the stages that write a file; `implement` has no such check and goes to review as today.
- **`testing` runs before the pull request,** so CI on the pull request is not part of it. CI stays visible in `release`, as today.
- **Later:**
  - Jira and Linear issue creation.
  - Comments on the issue at each stage.
  - The `url` source.
