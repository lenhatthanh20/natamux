import { z } from 'zod'
import { CONTEXT_SOURCE_TYPES } from './project-settings.js'
import { repoIdSchema } from './repo.js'
import { LEGACY_STEP_REFS, lanesSchema, stageIdSchema, stepRefSchema } from './stage.js'
import { taskStateSchema } from './task-state.js'

export const TASK_PRIORITIES = ['low', 'normal', 'high', 'urgent'] as const

export const PULL_REQUEST_STATES = ['open', 'draft', 'merged', 'closed'] as const
export const CHECK_STATES = ['passing', 'failing', 'pending', 'none'] as const

// What GitHub last said about the task's pull request and its checks (CI).
export const pullRequestStatusSchema = z.object({
  state: z.enum(PULL_REQUEST_STATES),
  checks: z.enum(CHECK_STATES),
})

export type PullRequestStatus = z.infer<typeof pullRequestStatusSchema>

// Where the context of a task came from, and what it was asked for: an issue number, a ticket key.
export const taskContextSchema = z.object({
  source: z.enum(CONTEXT_SOURCE_TYPES),
  ref: z.string().trim().max(500).default(''),
})

export type TaskContext = z.infer<typeof taskContextSchema>

// The item that tracks the task outside Natamux: a GitHub issue (#42), a Jira ticket (ABC-123).
export const issueLinkSchema = z.object({
  provider: z.string().trim().min(1).max(40),
  key: z.string().trim().min(1).max(100),
  url: z.url(),
})

export type IssueLink = z.infer<typeof issueLinkSchema>

export const taskIdSchema = z.string().regex(/^T-\d+$/, 'Task id looks like T-42')

export const taskSchema = z.object({
  id: taskIdSchema,
  title: z.string().trim().min(1).max(200),
  description: z.string().default(''),
  state: taskStateSchema.default('draft'),
  priority: z.enum(TASK_PRIORITIES).default('normal'),
  labels: z.array(z.string().min(1)).default([]),
  agent: z.string().min(1).nullable().default(null),
  branch: z.string().min(1).nullable().default(null),
  workspace: z.string().min(1).nullable().default(null),
  pr: z.url().nullable().default(null),
  prStatus: pullRequestStatusSchema.nullable().default(null),
  dependsOn: z.array(taskIdSchema).default([]),
  // The task this one is a part of. One level only: a subtask has no subtasks.
  parentId: taskIdSchema.nullable().default(null),
  // The repository of the task. Null is the daemon's own repository (DEFAULT_REPO).
  repo: repoIdSchema.nullable().default(null),
  // The current step: a stage id, or a group id while stages run side by side.
  stage: stageIdSchema.default('implement'),
  // The steps the task goes through, copied from its project when it was made.
  stages: z.array(stepRefSchema).min(1).default(LEGACY_STEP_REFS),
  // The state of each stage of the current group. Empty outside a group.
  lanes: lanesSchema.default({}),
  // When the current step started.
  stageSince: z.iso.datetime().nullable().default(null),
  // When the running agent ended its turn to wait for its background work, which wakes it again.
  // Null when it does not wait. A waiting task is not "quiet".
  backgroundSince: z.iso.datetime().nullable().default(null),
  context: taskContextSchema.nullable().default(null),
  issue: issueLinkSchema.nullable().default(null),
  createdAt: z.iso.datetime(),
  updatedAt: z.iso.datetime(),
})

export type Task = z.infer<typeof taskSchema>

const IMPORTED_ISSUE = /Imported from (https:\/\/github\.com\/[^\s]+\/issues\/(\d+))\s*$/

// Fills the stage fields of a task written before stages existed, from its state: a task in
// review is at the review stage, a done task finished its last stage, any other is implementing.
// An issue import line in its notes becomes its issue link.
export function upgradeTaskFields(raw: Record<string, unknown>): Record<string, unknown> {
  if (raw.stage !== undefined) {
    return raw
  }
  const state = raw.state
  const stage = state === 'review' ? 'review' : state === 'done' ? 'release' : 'implement'
  const imported =
    typeof raw.description === 'string' ? IMPORTED_ISSUE.exec(raw.description.trim()) : null
  return {
    ...raw,
    stage,
    stages: LEGACY_STEP_REFS,
    ...(raw.issue === undefined && imported?.[1] && imported[2]
      ? { issue: { provider: 'github', key: `#${imported[2]}`, url: imported[1] } }
      : {}),
  }
}
