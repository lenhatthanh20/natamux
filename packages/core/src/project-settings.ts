import { z } from 'zod'
import { DEFAULT_PIPELINE, pipelineSchema } from './stage.js'

export const CONTEXT_SOURCE_TYPES = ['manual', 'github-issue', 'skill', 'mcp'] as const
export type ContextSourceType = (typeof CONTEXT_SOURCE_TYPES)[number]

export const contextSourceSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('manual') }),
  z.object({ type: z.literal('github-issue') }),
  z.object({
    type: z.literal('skill'),
    name: z.string().trim().min(1).max(60),
    skill: z.string().trim().min(1),
    agent: z.string().min(1).optional(),
  }),
  z.object({
    type: z.literal('mcp'),
    name: z.string().trim().min(1).max(60),
    prompt: z.string().trim().min(1),
    agent: z.string().min(1).optional(),
  }),
])

export type ContextSource = z.infer<typeof contextSourceSchema>

export const CREATE_ISSUE_WHEN = ['never', 'manual', 'on-spec-approved', 'on-pr'] as const

export const trackerSettingsSchema = z.object({
  type: z.enum(['github', 'none']).default('github'),
  createIssue: z.enum(CREATE_ISSUE_WHEN).default('on-spec-approved'),
  linkInPr: z.boolean().default(true),
})

// `ask` passes no flag, so each agent keeps the user's own permission setting.
export const PERMISSION_MODES = ['auto', 'ask'] as const
export type PermissionMode = (typeof PERMISSION_MODES)[number]

export const agentSettingsSchema = z.object({
  permissionMode: z.enum(PERMISSION_MODES).default('auto'),
})

// `.natamux.json` at the root of a repository. Every field may be left out.
export const projectSettingsSchema = z.object({
  stages: pipelineSchema.default(DEFAULT_PIPELINE),
  context: z
    .object({
      sources: z
        .array(contextSourceSchema)
        .min(1, 'Give at least one context source')
        .default([{ type: 'manual' }, { type: 'github-issue' }]),
    })
    .default({ sources: [{ type: 'manual' }, { type: 'github-issue' }] }),
  tracker: trackerSettingsSchema.default({
    type: 'github',
    createIssue: 'on-spec-approved',
    linkInPr: true,
  }),
  agents: agentSettingsSchema.default({ permissionMode: 'auto' }),
})

export type ProjectSettings = z.infer<typeof projectSettingsSchema>

export const DEFAULT_PROJECT_SETTINGS: ProjectSettings = projectSettingsSchema.parse({})
