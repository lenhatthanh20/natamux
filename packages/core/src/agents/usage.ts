import { z } from 'zod'

// Tokens an agent used, in one form for every agent. `input` is fresh input only (no cache);
// `output` includes reasoning or thinking tokens.
export type TokenCounts = {
  input: number
  cacheRead: number
  cacheWrite: number
  output: number
}

const count = z.number().int().nonnegative()

// One stage, agent and model of a task. Stage "" is no stage. `costUsd` is null when the model has
// no price.
export const usageRowSchema = z.object({
  stage: z.string(),
  agent: z.string(),
  model: z.string(),
  input: count,
  cacheRead: count,
  cacheWrite: count,
  output: count,
  costUsd: z.number().nonnegative().nullable(),
})

export type UsageRow = z.infer<typeof usageRowSchema>

// What a task used, as GET /usage sends it. USD is an estimate at the API list price. `costUsd` is
// null when no row has a price, else the sum of the priced rows.
export const taskUsageSchema = z.object({
  tokens: count,
  costUsd: z.number().nonnegative().nullable(),
  unpricedModels: z.array(z.string()),
  stages: z.array(usageRowSchema),
})

export type TaskUsage = z.infer<typeof taskUsageSchema>
