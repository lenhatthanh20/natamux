---
title: Configuration
description: Reference for the .natamux.json file that sets the stages, context sources, issue tracker and agent permission mode of a repository.
sidebar:
  order: 2
---

Natamux reads project settings from a file named `.natamux.json` at the root of a repository. The file sets the [stages](/docs/concepts/stages/) that tasks go through, where task context comes from, and how Natamux works with your issue tracker.

Every field is optional. A repository without the file uses the defaults on this page. To set daemon options such as the port and data folder, see [Environment variables](/docs/reference/environment-variables/).

## How the file is loaded

- The file is read again when it changes. An edit counts at once.
- A new task takes the stages from the file when you create it. Tasks on their way keep their list of stages. Changes to the name, gate and checks of a stage apply to them.
- If the file is not valid JSON, or a field is wrong, Natamux uses the defaults and shows the problem. Fix the file to use your settings.
- Unknown fields are ignored.
- Each repository that Natamux manages has its own file.

To edit the file in the dashboard, open **Settings** in the menu of the project. The **Stages** tab edits the stages without JSON, the **Issues** tab sets when an issue opens, and the **JSON** tab edits the whole file. **Save** writes `.natamux.json` in the repository. Commit it to share it with your team. **Reset to default** puts the default pipeline in the editor. See [Project settings](/docs/guides/project-settings/).

- The editor changes only `stages`. Other fields and keys it does not know stay in the file as they are.
- When the stages equal the default pipeline, the editor leaves `stages` out of the file, so the project follows later changes of the default.

## Top-level fields

| Field | Type | Default | Meaning |
|---|---|---|---|
| `stages` | array of stages, groups and branches | The [default pipeline](#default-pipeline) | The steps of every task, in order. At least one. |
| `context` | object | `{ "sources": [{ "type": "manual" }, { "type": "github-issue" }] }` | Where task context can come from. |
| `tracker` | object | `{ "type": "github", "createIssue": "manual", "linkInPr": true }` | The issue tracker. |
| `agents` | object | `{ "permissionMode": "auto" }` | How the agents that Natamux starts ask for permission, and how often they may send a task back. |
| `git` | object | `{}` | How Natamux names the branch of a new workspace. |

## `stages`

An array of **steps**. A step is a stage, a group of stages that run side by side, or a branch that splits the pipeline into paths.

### Stage

| Field | Type | Required | Default | Meaning |
|---|---|---|---|---|
| `id` | string | yes | | Lower case words joined by `-`, such as `test-spec`. Up to 40 characters. Unique in the file. |
| `kind` | string | yes | | `context`, `agent`, `review`, `testing`, `release` or `manual`. |
| `gate` | string | yes | | `auto` or `manual`. |
| `name` | string | no | The `id` with spaces and a capital, such as `Test spec` | Display name, 1 to 60 characters. |
| `prompt` | string | no | A built-in prompt for the default ids, or a generic one | The prompt for the agent. |
| `output` | string | no | none | A Markdown file the stage must write, such as `spec.md`. |
| `sections` | array of strings | no | none | Headings the output file must have. |
| `requireCommit` | boolean | no | `false` | Fail the check when there is no new commit. |
| `requireClean` | boolean | no | `false` | Fail the check when a file is not committed. |
| `check` | string | no | none | A shell command that must exit with 0. |
| `command` | string | no | none | For `testing`: the command to run. Its exit code decides the stage. |
| `doneWhen` | string | no | A sentence made from the checks | Text shown in the progress view before the stage starts. |
| `enabled` | boolean | no | `true` | `false` turns the stage off: it stays in the file, but new tasks do not get it. |
| `workflow` | boolean | no | `true` for `plan` and `implement` of the catalog, else `false` | Tasks get a **Run with a workflow** checkbox on this stage. Only for `agent` stages. See [Run a stage as a workflow](/docs/concepts/stages/#run-a-stage-as-a-workflow). |
| `agent` | string | no | The task's agent | The agent this stage starts: `claude-code`, `codex`, `gemini`, `copilot` or `cursor`. Only on an `agent` or `testing` stage, of the catalog or your own. |
| `ci` | boolean | no | `true` for `implement` and `testing` of the catalog, else `false` | Tasks get a **Check on CI** checkbox on this stage. Only for a testing stage or an agent stage without an `output`. See [Check a stage on CI](/docs/concepts/stages/#check-a-stage-on-ci). |
| `ownShell` | boolean | no | `true` for the catalog's report stages (Review, Test spec, Security review, Performance check, Accessibility check) | The stage runs in its own terminal (Terminal 5) with a new session, or in a terminal of its own from Terminal 6 when it has its own `agent` or `args`. The stage then does not use Terminal 1, so the task's agent starts there only at the next stage that does. Only for an `agent` stage with an `output`. |
| `endShell` | boolean | no | `false` | When the stage is done or sends the task back, end its agent or command and close its terminal. Without it, the terminal stays open until the task ends or you close it. Only for `context`, `agent` and `testing` stages. Ignored on a stage in Terminal 1, which is never closed. See [End a stage's terminal](/docs/concepts/stages/#end-a-stages-terminal). |
| `args` | string | no | none | Words added to the command line of this stage's agent, after `NATAMUX_<AGENT>_ARGS`, such as `--model sonnet`. Split at spaces, each word quoted, so a quoted value with spaces becomes several words. Left out until the project's commands are approved. Same rule as `agent`. |

Details:

- `kind` and `gate` are explained in [Stages](/docs/concepts/stages/).
- `output` is a file name in `.natamux-task/`. It has letters, digits, `_`, `-` and `.`, and ends in `.md`. It has no folder.
- `prompt` can use `{title}`, `{description}`, `{dir}`, `{ref}` and `{skill:<name>}`. `{dir}` is the stage file folder (`.natamux-task`). `{ref}` is the context reference of the task. Natamux adds the line `When you are done, run natamux done, unless the steps above say not to.` after your text, so your prompt can say when the agent must not run it, for example after it sends the task back.
- `{skill:<name>}` names a skill. A plugin skill is `<plugin>:<skill>`. Natamux fills it when the stage starts, for the agent of the task. See [Skill placeholder](#skill-placeholder).
- A stage with `"enabled": false` keeps its settings in the file. The pipeline rules below count only stages that are on.
- `check` runs in the root of the worktree. It gets 10 minutes. It runs with `sh -c`, or `cmd.exe` on Windows.
- `command` only has an effect on a `testing` stage. Without it, an agent does the testing.
- `agent` and `args` are for an `agent` or `testing` stage, wherever it runs, whether it comes from the [catalog](#stage-catalog) or is your own. Such a stage starts its own agent in its own terminal, so it can use another agent or model without touching the session in Terminal 1. A launch for such a stage does not change the task's agent.
- A stage with its own `agent` or `args` runs in a terminal of its own: Terminal 6 plus the place of the stage among all stages of the task, counted from 0, so no two stages share one. Natamux closes no agent while the task runs, unless its stage has `"endShell": true`.
- A stage keeps the terminal it started in until it starts again. A change to these settings applies at the stage's next start, never under an agent that runs. See [Choose a model](/docs/concepts/stages/#choose-a-model).
- `workflow` only offers the choice. Each task turns it on for itself; it is off by default. A stage of your own does not take the default, so write `"workflow": true` on it to offer the choice.

### Group

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | Same rules as a stage id. Unique, also against stage ids inside groups. |
| `name` | string | no | Display name, 1 to 60 characters. |
| `parallel` | array of stages | yes | 2 to 4 stages that run at the same time. Only kinds `agent` and `manual`. |

### Branch

A branch splits the pipeline into 2 to 4 paths. A task takes the first path whose condition holds. See [Paths with a condition](/docs/concepts/stages/#paths-with-a-condition).

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | Same rules as a stage id. Unique, also against stage ids and path ids. |
| `name` | string | no | Display name, 1 to 60 characters. |
| `paths` | array of paths | yes | 2 to 4 paths, checked in order. |

### Path

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | Same rules as a stage id. Unique in the file. |
| `name` | string | no | Display name, 1 to 60 characters. |
| `when` | condition | no | What must hold to take the path. Leave it out for the **Otherwise** path. |
| `not` | boolean | no | `true` takes the path when the condition does not hold. |
| `stages` | array of stages | yes | At least one. The stages run one after another. A path holds stages only: no group and no branch. |

### Condition

The `type` of `when` picks the fields.

| Type | Field | Required | Meaning |
|---|---|---|---|
| `output` | `file` | yes | A Markdown file name in the task's stage folder, such as `plan.md`. |
| `output` | `section` | no | A heading. Only the lines under it count. |
| `output` | `contains` | yes | The text to look for. The match ignores case. |
| `command` | `run` | yes | A shell command. The condition holds when it exits with 0. |

```json title=".natamux.json"
{
  "id": "after-plan",
  "paths": [
    {
      "id": "small",
      "when": { "type": "output", "file": "plan.md", "section": "Size", "contains": "small" },
      "stages": [{ "id": "implement", "kind": "agent", "gate": "auto" }]
    },
    {
      "id": "pr-review",
      "when": { "type": "command", "run": "test -f .no-review" },
      "not": true,
      "stages": [{ "id": "review", "kind": "review", "gate": "manual" }]
    }
  ]
}
```

- The check happens when the stage before the branch ends. For a stage that waits for approval, that is when you approve.
- A `command` runs in the worktree with the shell of your system, and stops after 10 minutes. A command that fails to start or times out means the condition does not hold.
- A missing file means the condition does not hold.
- Only the last path can leave out `when`. An earlier path without `when` is an error.
- No path holds and there is no Otherwise path: the task skips the branch.

### Rules for a branch

- A branch has 2 to 4 paths, and each path has at least one stage.
- A branch cannot be the first step.
- A branch cannot come right after another branch. Put a stage between them.
- A path cannot hold a `context` or `release` stage, a group or another branch.
- Every stage id and path id is used once in the whole file.
- A path with all its stages off is left out of new tasks. If only one path is left and it has a condition, the file is an error: turn on a stage in another path, or remove the branch. If only the Otherwise path is left, its stages run as plain steps.
- A path keeps the same rules for stages as a plain step: `ownShell`, `agent`, `args`, `ci`, `endShell` and `workflow`.

### Rules for the pipeline

- At least one step.
- Every id is used once, counting the stages inside groups and paths.
- A `context` stage must be the first step. There is at most one.
- A `release` stage must be the last step. There is at most one.
- A `context` or `release` stage cannot be inside a group.
- These rules count only stages that are on. A group needs 2 stages that are on. At least one stage must be on.
- When a task starts, a group with one stage on runs that stage on its own, and a group with none is left out.
- `"workflow": true` is only allowed on an `agent` stage.
- `agent` and `args` are only allowed on an `agent` stage in a group that does not keep Terminal 1, on one with `"ownShell": true`, or on an `agent` or `testing` stage of your own.
- `"ownShell": true` is only allowed on an `agent` stage with an `output`.
- `"ci": true` is only allowed on a `testing` stage or an `agent` stage without an `output`.
- `"endShell": true` is only allowed on a `context`, `agent` or `testing` stage. On a stage in Terminal 1 it is ignored.

### Skill placeholder

`{skill:<name>}` becomes text for the agent of the task:

| Case | Text |
|---|---|
| The agent has the skill and a sure call: Claude Code and Copilot CLI (`/name`), Codex (`$name`) | The call, such as `/review-pr` |
| A `SKILL.md` with that name exists for any agent, in the worktree or in the main checkout (Gemini CLI and Cursor CLI, and a skill found only in the main checkout, always get this) | `the skill in <path> (read that file and follow it)` |
| No such skill | `the <name> skill`. The stage card says "The skill <name> was not found for <agent>." |

The stage starts in every case. Each agent page says where it finds skills, for example [Codex](/docs/agents/codex/#skills).

### Stage catalog

**Add stage** in the dashboard picks from this catalog. A stage has the kind, gate and checks below when you add it. Gate `manual` is **Wait for my approval**. The stages marked "default" make up the [default pipeline](#default-pipeline).

| Phase | Id | Name | Kind | Gate | Output | What it does |
|---|---|---|---|---|---|---|
| Understand | `context` | Context | `context` | manual | `context.md` | Default. Reads the context sources of the task. |
| Understand | `research` | Research | `agent` | manual | `research.md` | Reads the code and docs the task touches. |
| Understand | `clarify` | Open questions | `agent` | manual | `questions.md` | Lists what is unclear, with a suggested answer for each. |
| Design | `spec` | Spec | `agent` | manual | `spec.md` | Default. Goal, scope, acceptance criteria and a code map. |
| Design | `ux-design` | UX design | `agent` | manual | `ux.md` | Screens, states, flows, accessibility. |
| Design | `architecture` | Architecture | `agent` | manual | `architecture.md` | Components, data flow, interfaces, trade-offs. |
| Design | `threat-model` | Threat model | `agent` | manual | `threat-model.md` | What an attacker could reach, and how each threat is handled. |
| Design | `plan` | Plan | `agent` | manual | `plan.md` | Default. Steps, files and tests. Offers a workflow. |
| Build | `test-spec` | Test spec | `agent` | manual | `test-spec.md` | Default. Test cases from the spec, without reading the code. |
| Build | `implement` | Implement | `agent` | auto | none | Default. Tests first, then the code. Offers a workflow. |
| Build | `migration` | Data migration | `agent` | manual | none | A data or schema migration, with a way to roll it back. |
| Build | `refactor` | Refactor | `agent` | auto | none | Cleans up the changed code without changing what it does. |
| Build | `docs` | Documentation | `agent` | auto | none | Updates the user and developer docs. |
| Build | `i18n` | Translations | `agent` | auto | none | Adds or updates the translations of new UI text. |
| Check | `lint` | Lint and types | `testing` | auto | none | Runs your lint and type check command. Needs a `command`. |
| Check | `self-review` | Agent self-review | `agent` | auto | `self-review.md` | Reads the diff against the spec, lists problems and fixes them. |
| Check | `security` | Security review | `agent` | manual | `security.md` | Checks the diff for security problems. |
| Check | `performance` | Performance check | `agent` | manual | `performance.md` | Looks for slow paths in the diff. |
| Check | `accessibility` | Accessibility check | `agent` | manual | `accessibility.md` | Checks UI changes for keyboard, screen reader and contrast problems. |
| Check | `agent-review` | Review | `agent` | auto | `review.md` | Default. An agent reviews the changes against the spec in its own terminal, and sends the task back to Implement when they need fixes. |
| Check | `review` | Manual review | `review` | manual | none | A person reads the diff and approves it or sends it back. |
| Check | `testing` | Automated tests | `testing` | auto | none | Default. An agent adds a test for each case of the test spec and runs all tests, or a `command` runs them. |
| Check | `e2e` | End-to-end tests | `testing` | auto | none | Adds end-to-end tests, then runs them. Needs a `command`. |
| Check | `qa` | Manual testing | `manual` | manual | none | A person tries the change by hand. |
| Check | `design-review` | Design review | `manual` | manual | none | A designer checks the UI. |
| Ship | `changelog` | Release notes | `agent` | auto | none | Adds a changelog entry. |
| Ship | `staging` | Staging check | `manual` | manual | none | You check the change on a staging server. |
| Ship | `release` | Release | `release` | manual | none | Default. Pushes the branch and opens a pull request. |

- The stages with an output have `sections` for the checks. The `review`, `manual` and `release` stages always wait for a person or an outside event, whatever the gate says.
- `lint` and `e2e` run your own command, so the screen asks for a **Command**. Without one, an agent does the work.
- A catalog stage that is gone from the file keeps its catalog settings for a task on its way.

### Default pipeline

```json
{
  "stages": [
    { "id": "context", "kind": "context", "gate": "manual", "output": "context.md" },
    {
      "id": "spec",
      "kind": "agent",
      "gate": "manual",
      "output": "spec.md",
      "sections": ["Goal", "Scope", "Out of scope", "Acceptance criteria", "Code map"]
    },
    {
      "id": "plan",
      "kind": "agent",
      "gate": "manual",
      "output": "plan.md",
      "sections": ["Steps", "Files", "Tests"],
      "workflow": true
    },
    {
      "id": "build",
      "name": "Test spec and Implement",
      "parallel": [
        {
          "id": "test-spec",
          "kind": "agent",
          "gate": "manual",
          "output": "test-spec.md",
          "sections": ["Cases"],
          "ownShell": true
        },
        { "id": "implement", "kind": "agent", "gate": "auto", "workflow": true, "ci": true }
      ]
    },
    { "id": "testing", "name": "Automated tests", "kind": "testing", "gate": "auto", "ci": true },
    {
      "id": "agent-review",
      "name": "Review",
      "kind": "agent",
      "gate": "auto",
      "output": "review.md",
      "sections": ["Findings"],
      "ownShell": true
    },
    { "id": "release", "kind": "release", "gate": "manual" }
  ]
}
```

## Limits

| What | Limit |
|---|---|
| Stages in a group | 2 to 4. Only `agent` and `manual` stages. |
| Paths in a branch | 2 to 4. |
| Stage id, path id, group id | Up to 40 characters, lower case words joined by `-`. |
| Display name | 1 to 60 characters. |
| Sends back by a review agent from one stage | 2 per task. The third time the stage waits as **Needs input**. A person can send back as often as they like. |
| Sends back by CI from one stage | 2 per task, shared with the review agent. The third red run waits for you. |
| Wait for a CI run to start | 3 minutes. |
| Wait for a CI run to finish | 30 minutes. |
| A `check` command, and a `command` of a condition | 10 minutes each. |
| Terminals per task | 6. Natamux runs 64 terminals in all. |
| Shell for commands | `sh -c`, or `cmd.exe` on Windows. |
| Attachments per task | 20 files, 20 MB each, 50 MB in all. |

## `context`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `sources` | array of sources | `manual` and `github-issue` | The ways a task can get its context. At least one. |

When you create a task, you pick one source with its type, for example `--context github-issue:42`. If the file does not list that type, Natamux uses `manual`.

### Source types

| Type | Fields | Meaning |
|---|---|---|
| `manual` | `type` | The title, notes and reference of the task become the context. |
| `github-issue` | `type` | Natamux reads a GitHub issue with `gh`. |
| `skill` | `type`, `name`, `skill`, `agent` (optional) | An agent runs the skill and writes the context. Natamux names the skill the way the agent of the task needs. |
| `mcp` | `type`, `name`, `prompt`, `agent` (optional) | An agent runs your `prompt`, for example to read a ticket through an MCP server. |

- `name` is 1 to 60 characters.
- `skill` is the name of the skill, without the slash or the dollar sign.
- A `skill` source is optional. **Start a task** offers every skill of the agents as **An agent skill**. Configure one to give it a name in **Context from**.
- A `skill` source follows the three rules of the [skill placeholder](#skill-placeholder), as a whole line. For the skill `fetch-ticket` and the reference `ABC-1`, an agent with the skill gets `/fetch-ticket ABC-1` (a call). Otherwise it gets `Read <path> and follow it for ABC-1.` (the file), or `Use the fetch-ticket skill for ABC-1.` (not found). The file format does not change.
- `prompt` can use `{title}`, `{description}`, `{dir}` and `{ref}`.
- The `agent` field is accepted, but Natamux does not use it yet. The task's own agent runs a `skill` or `mcp` source, or `claude-code` when the task has none.

```json
{
  "context": {
    "sources": [
      { "type": "manual" },
      { "type": "github-issue" },
      { "type": "mcp", "name": "Jira", "prompt": "Read ticket {ref} with the Jira MCP server. Write a summary to {dir}/context.md." }
    ]
  }
}
```

## `tracker`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `type` | `github` or `none` | `github` | The issue tracker. `none` turns off issue creation. |
| `createIssue` | string | `manual` | When Natamux opens an issue for a task. |
| `linkInPr` | boolean | `true` | Add a closing line to the body of the pull request. |

Values of `createIssue`:

| Value | Natamux opens an issue |
|---|---|
| `never` | Never. The **Create issue** button is hidden. |
| `manual` (default) | Only when you ask for it (**Create issue**). |
| `on-spec-approved` | When you approve the `spec` stage. In a pipeline with no stage called `spec`, when the pull request is opened. |
| `on-pr` | When the pull request is opened. |

The issue gets the task title. Its body is the spec, or the task notes without one. With `linkInPr`, the pull request body ends with `Closes #42` for a GitHub issue, so the issue closes on merge. See [GitHub](/docs/guides/github/).

## `agents`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `permissionMode` | `auto` or `ask` | `auto` | `auto` starts each agent with the flag below, so it asks for permission less often. `ask` adds no flag: each agent follows your own settings. |
| `maxSendBacks` | whole number, 0 or more | none (no limit) | The most times an agent, or a CI failure, may send the task back from one stage since you last stepped in. Then the stage waits for you. `0`: an agent never sends back by itself. See [The review loop](/docs/concepts/review-loop/#limits). |
| `pauseIdleAfterMinutes` | whole number, `0` or 5 to 1440 | `30` | After how many idle minutes Natamux pauses an agent that ended its turn in a terminal nobody looks at. It ends the terminal and keeps the conversation. `0`: never. See [An idle agent pauses](/docs/guides/terminal/#an-idle-agent-pauses). |

The flag of each agent in `auto`:

| Agent | Flag | What it allows |
|---|---|---|
| Claude Code | `--permission-mode auto` | Claude Code checks each action and asks only when one looks risky. |
| Codex | `--sandbox workspace-write --ask-for-approval never` | Edits and commands inside its sandbox of the worktree, without asking. |
| Gemini CLI | `--approval-mode yolo` | Every tool call, without asking. |
| Copilot CLI | `--allow-all-tools` | Every tool, without asking. It still checks paths and URLs. |
| Cursor CLI | `--force` | Every command that is not denied in its settings, without asking. |

Only Claude Code has a check on each action in this mode. The other agents run commands without asking, in the task's worktree with your user rights. Use `ask` for a repository where that is not safe:

```json
{ "agents": { "permissionMode": "ask" } }
```

The flag comes before the words of `NATAMUX_<AGENT>_ARGS`. When those words repeat or change the flag, the agent's own rule for flags decides. The setting counts for the next agent Natamux starts, not for one that runs.

To stop two agents that disagree from sending a task back and forth, set a limit. It counts for the next send back, also for a task on its way:

```json
{ "agents": { "maxSendBacks": 2 } }
```

To keep idle agents running, turn the pause off:

```json
{ "agents": { "pauseIdleAfterMinutes": 0 } }
```

## `git`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `branchName` | string | `natamux/{id}-{slug}` | The name of the branch of a new workspace. See [Branch name](/docs/concepts/workspaces/#branch-name). |

Placeholders:

| Placeholder | Value |
|---|---|
| `{id}` | The task id as it is, such as `ABC-12` or `T-0042` |
| `{slug}` | The slug of the title, such as `fix-login` |

Rules:

- Any other `{...}` is a problem: `git.branchName: Unknown placeholder {x}`.
- At most 255 characters.
- Not empty.
- The filled name must be a valid branch name. If not, the workspace fails with `Branch name <name> is not valid`.

```json
{ "git": { "branchName": "{id}" } }
```

Without `branchName`, the name is `natamux/` + the id in lower case + the slug.

## Examples

### Drop the planning stages

Go from context straight to code. Remove `spec`, `plan` and the group, and add one `implement` stage. With `requireCommit` and `requireClean` the agent commits its work. Without them it leaves the changes for you to commit:

```json
{
  "stages": [
    { "id": "context", "kind": "context", "gate": "manual", "output": "context.md" },
    { "id": "implement", "kind": "agent", "gate": "auto", "requireCommit": true, "requireClean": true },
    { "id": "review", "kind": "review", "gate": "manual" },
    { "id": "release", "kind": "release", "gate": "manual" }
  ]
}
```

### Run a test command

Run your tests in a shell. The stage passes when the command exits with 0:

```json
{
  "stages": [
    { "id": "implement", "kind": "agent", "gate": "auto", "requireCommit": true },
    { "id": "review", "kind": "review", "gate": "manual" },
    { "id": "testing", "kind": "testing", "gate": "auto", "command": "pnpm test" },
    { "id": "release", "kind": "release", "gate": "manual" }
  ]
}
```

### Check a command after an agent stage

Make a stage fail until a command passes. Here the `implement` stage needs a commit and a passing lint:

```json
{
  "stages": [
    {
      "id": "implement",
      "kind": "agent",
      "gate": "auto",
      "requireCommit": true,
      "check": "pnpm lint"
    },
    { "id": "review", "kind": "review", "gate": "manual" }
  ]
}
```

### Run stages side by side

Write the docs while the agent writes the code. Both stages start together, each in its own terminal:

```json
{
  "stages": [
    {
      "id": "build",
      "name": "Code and docs",
      "parallel": [
        { "id": "implement", "kind": "agent", "gate": "auto", "requireCommit": true },
        {
          "id": "docs",
          "kind": "agent",
          "gate": "manual",
          "output": "docs-notes.md",
          "sections": ["Changes"],
          "prompt": "Task: {title}\nWrite {dir}/docs-notes.md with a Changes section that lists what the docs must say. Do not change any code."
        }
      ]
    },
    { "id": "review", "kind": "review", "gate": "manual" }
  ]
}
```

### Add a manual sign-off

A `manual` stage waits for your approval. Use `doneWhen` to tell people what it needs:

```json
{ "id": "legal", "kind": "manual", "gate": "manual", "doneWhen": "Legal signs off." }
```

### Review with another agent

Let Codex review the work of the task's agent. Copy the [default pipeline](#default-pipeline) and add `agent` and `args` to the Review stage:

```json
{
  "id": "agent-review",
  "name": "Review",
  "kind": "agent",
  "gate": "auto",
  "output": "review.md",
  "sections": ["Findings"],
  "ownShell": true,
  "agent": "codex",
  "args": "--model gpt-5"
}
```

**Agent** and **Arguments** in [Project settings](/docs/guides/project-settings/#choose-the-review-agent) write the same fields.

### Turn off issues

```json
{ "tracker": { "type": "none" } }
```

## Related

- [Stages](/docs/concepts/stages/): how kinds, gates and checks work
- [Project settings](/docs/guides/project-settings/): edit the settings on a screen
- [The review loop](/docs/concepts/review-loop/): the default pipeline at work
- [HTTP API](/docs/reference/http-api/): read and write the settings from a script
