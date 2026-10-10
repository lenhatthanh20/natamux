---
title: Configuration
description: Reference for the .natamux.json file that sets the stages, context sources and issue tracker of a repository.
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

To edit the file in the dashboard, open the project settings of the repository. Save writes `.natamux.json` in the repository. Commit it to share it with your team. **Reset to default** puts the default settings in the editor.

## Top-level fields

| Field | Type | Default | Meaning |
|---|---|---|---|
| `stages` | array of stages and groups | The [default pipeline](#default-pipeline) | The steps of every task, in order. At least one. |
| `context` | object | `{ "sources": [{ "type": "manual" }, { "type": "github-issue" }] }` | Where task context can come from. |
| `tracker` | object | `{ "type": "github", "createIssue": "on-spec-approved", "linkInPr": true }` | The issue tracker. |

## `stages`

An array of **steps**. A step is a stage, or a group of stages that run side by side.

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

Details:

- `kind` and `gate` are explained in [Stages](/docs/concepts/stages/).
- `output` is a file name in `.natamux-task/`. It has letters, digits, `_`, `-` and `.`, and ends in `.md`. It has no folder.
- `prompt` can use `{title}`, `{description}`, `{dir}` and `{ref}`. `{dir}` is the stage file folder (`.natamux-task`). `{ref}` is the context reference of the task. Natamux adds the line `When you are done, run natamux done.` after your text.
- `check` runs in the root of the worktree. It gets 10 minutes. It runs with `sh -c`, or `cmd.exe` on Windows.
- `command` only has an effect on a `testing` stage. Without it, an agent does the testing.

### Group

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | Same rules as a stage id. Unique, also against stage ids inside groups. |
| `name` | string | no | Display name, 1 to 60 characters. |
| `parallel` | array of stages | yes | 2 to 4 stages that run at the same time. Only kinds `agent` and `manual`. |

### Rules for the pipeline

- At least one step.
- Every id is used once, counting the stages inside groups.
- A `context` stage must be the first step. There is at most one.
- A `release` stage must be the last step. There is at most one.
- A `context` or `release` stage cannot be inside a group.

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
      "sections": ["Goal", "Scope", "Out of scope", "Acceptance criteria"]
    },
    {
      "id": "plan",
      "kind": "agent",
      "gate": "manual",
      "output": "plan.md",
      "sections": ["Steps", "Files", "Tests"]
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
          "sections": ["Cases"]
        },
        {
          "id": "implement",
          "kind": "agent",
          "gate": "auto",
          "requireCommit": true,
          "requireClean": true
        }
      ]
    },
    { "id": "review", "kind": "review", "gate": "manual" },
    { "id": "testing", "kind": "testing", "gate": "auto" },
    { "id": "release", "kind": "release", "gate": "manual" }
  ]
}
```

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
| `skill` | `type`, `name`, `skill`, `agent` (optional) | An agent runs the skill `/<skill> <ref>` and writes the context. |
| `mcp` | `type`, `name`, `prompt`, `agent` (optional) | An agent runs your `prompt`, for example to read a ticket through an MCP server. |

- `name` is 1 to 60 characters.
- `skill` is the name of the skill, without the slash.
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
| `createIssue` | string | `on-spec-approved` | When Natamux opens an issue for a task. |
| `linkInPr` | boolean | `true` | Add a closing line to the body of the pull request. |

Values of `createIssue`:

| Value | Natamux opens an issue |
|---|---|
| `never` | Never. |
| `manual` | Only when you ask for it (**Create issue**). |
| `on-spec-approved` | When you approve the `spec` stage. In a pipeline with no stage called `spec`, when the pull request is opened. |
| `on-pr` | When the pull request is opened. |

The issue gets the task title. Its body is the spec, or the task notes without one. With `linkInPr`, the pull request body ends with `Closes #42` for a GitHub issue, so the issue closes on merge. See [GitHub](/docs/guides/github/).

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

Write the docs while the agent writes the code. Both stages start together, each in its own shell:

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

### Turn off issues

```json
{ "tracker": { "type": "none" } }
```

## Related

- [Stages](/docs/concepts/stages/): how kinds, gates and checks work
- [HTTP API](/docs/reference/http-api/): read and write the settings from a script
