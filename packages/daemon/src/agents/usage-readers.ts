import { createReadStream } from 'node:fs'
import { readdir, readFile } from 'node:fs/promises'
import { basename, dirname, join } from 'node:path'
import { createInterface } from 'node:readline'
import type { TokenCounts } from '@natamux/core'

// `cacheWrite1h` is the part of `cacheWrite` that has the long cache's own price.
export type UsageCounts = TokenCounts & { cacheWrite1h: number }

// A session's usage so far, by model.
export type ModelUsage = Record<string, UsageCounts>

// Reads a session file. Only numbers and model names leave it; a bad line is skipped.
export type UsageReader = (path: string) => Promise<ModelUsage>

const UNKNOWN_MODEL = 'unknown'

export const COUNT_FIELDS = ['input', 'cacheRead', 'cacheWrite', 'cacheWrite1h', 'output'] as const

export function zeroCounts(): UsageCounts {
  return { input: 0, cacheRead: 0, cacheWrite: 0, cacheWrite1h: 0, output: 0 }
}

export function addCounts(usage: ModelUsage, model: string, counts: UsageCounts): void {
  const total = usage[model] ?? zeroCounts()
  for (const field of COUNT_FIELDS) {
    total[field] += counts[field]
  }
  usage[model] = total
}

type Json = Record<string, unknown>

function object(value: unknown): Json | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Json)
    : undefined
}

function count(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? Math.floor(value) : 0
}

function modelOf(value: unknown): string {
  return typeof value === 'string' && value.trim() !== '' ? value.trim() : UNKNOWN_MODEL
}

async function* jsonLines(path: string): AsyncGenerator<Json> {
  const lines = createInterface({ input: createReadStream(path, 'utf8'), crlfDelay: Infinity })
  for await (const line of lines) {
    try {
      const value = object(JSON.parse(line))
      if (value !== undefined) {
        yield value
      }
    } catch {
      // A broken line, such as the last one while the agent still writes it.
    }
  }
}

// Claude Code writes one message on several lines, each with the usage so far: the last one counts.
async function readClaudeFile(path: string, byMessage: Map<string, [string, UsageCounts]>) {
  let line = 0
  for await (const entry of jsonLines(path)) {
    line += 1
    const message = object(entry.message)
    const usage = object(message?.usage)
    // "<synthetic>" marks a message that Claude Code made itself, without a model call.
    if (message === undefined || usage === undefined || message.model === '<synthetic>') {
      continue
    }
    const id = typeof message.id === 'string' ? message.id : `${path}:${line}`
    byMessage.set(id, [
      modelOf(message.model),
      {
        input: count(usage.input_tokens),
        cacheRead: count(usage.cache_read_input_tokens),
        cacheWrite: count(usage.cache_creation_input_tokens),
        cacheWrite1h: count(object(usage.cache_creation)?.ephemeral_1h_input_tokens),
        output: count(usage.output_tokens),
      },
    ])
  }
}

// The session's file, plus its sub-agents in <session>/subagents/*.jsonl next to it.
export async function readClaudeUsage(path: string): Promise<ModelUsage> {
  const byMessage = new Map<string, [string, UsageCounts]>()
  await readClaudeFile(path, byMessage)
  const subagents = join(dirname(path), basename(path, '.jsonl'), 'subagents')
  const entries = await readdir(subagents, { withFileTypes: true }).catch(() => [])
  for (const entry of entries) {
    if (entry.isFile() && entry.name.endsWith('.jsonl')) {
      await readClaudeFile(join(subagents, entry.name), byMessage)
    }
  }
  const usage: ModelUsage = {}
  for (const [model, counts] of byMessage.values()) {
    addCounts(usage, model, counts)
  }
  return usage
}

// Codex's input_tokens include the cached and cache-write tokens; output_tokens include reasoning.
function codexCounts(total: Json): UsageCounts {
  const cacheRead = count(total.cached_input_tokens)
  const cacheWrite = count(total.cache_write_input_tokens)
  return {
    input: Math.max(0, count(total.input_tokens) - cacheRead - cacheWrite),
    cacheRead,
    cacheWrite,
    cacheWrite1h: 0,
    output: count(total.output_tokens),
  }
}

// Codex writes the session's running total after each model call, sometimes twice. Each change of
// the total goes to the model of the latest turn.
export async function readCodexUsage(path: string): Promise<ModelUsage> {
  const usage: ModelUsage = {}
  let model = UNKNOWN_MODEL
  let last = zeroCounts()
  for await (const entry of jsonLines(path)) {
    const payload = object(entry.payload)
    if (entry.type === 'turn_context' && typeof payload?.model === 'string') {
      model = modelOf(payload.model)
      continue
    }
    const total = object(object(payload?.info)?.total_token_usage)
    if (entry.type !== 'event_msg' || payload?.type !== 'token_count' || total === undefined) {
      continue
    }
    const now = codexCounts(total)
    const change = zeroCounts()
    for (const field of COUNT_FIELDS) {
      change[field] = Math.max(0, now[field] - last[field])
    }
    last = now
    addCounts(usage, model, change)
  }
  return dropEmpty(usage)
}

// Copilot's inputTokens include cache reads and writes.
export async function readCopilotUsage(path: string): Promise<ModelUsage> {
  const usage: ModelUsage = {}
  for await (const entry of jsonLines(path)) {
    const record = object(object(entry.data)?.usage)
    if (entry.type !== 'session.usage_record' || record === undefined) {
      continue
    }
    const cacheRead = count(record.cacheReadTokens)
    const cacheWrite = count(record.cacheWriteTokens)
    addCounts(usage, modelOf(record.model), {
      input: Math.max(0, count(record.inputTokens) - cacheRead - cacheWrite),
      cacheRead,
      cacheWrite,
      cacheWrite1h: 0,
      output: count(record.outputTokens),
    })
  }
  return usage
}

// One JSON file. A message's `input` includes `cached`; its `thoughts` are output.
export async function readGeminiUsage(path: string): Promise<ModelUsage> {
  const usage: ModelUsage = {}
  const messages = object(JSON.parse(await readFile(path, 'utf8')))?.messages
  for (const message of Array.isArray(messages) ? messages : []) {
    const tokens = object(object(message)?.tokens)
    if (tokens === undefined) {
      continue
    }
    const cacheRead = count(tokens.cached)
    addCounts(usage, modelOf(object(message)?.model), {
      input: Math.max(0, count(tokens.input) - cacheRead),
      cacheRead,
      cacheWrite: 0,
      cacheWrite1h: 0,
      output: count(tokens.output) + count(tokens.thoughts),
    })
  }
  return usage
}

function dropEmpty(usage: ModelUsage): ModelUsage {
  return Object.fromEntries(
    Object.entries(usage).filter(([, counts]) => COUNT_FIELDS.some((field) => counts[field] > 0)),
  )
}

// Cursor writes no usage, so it has no reader.
const READERS: Record<string, UsageReader> = {
  'claude-code': readClaudeUsage,
  codex: readCodexUsage,
  copilot: readCopilotUsage,
  gemini: readGeminiUsage,
}

export function usageReader(agent: string): UsageReader | undefined {
  return READERS[agent]
}
