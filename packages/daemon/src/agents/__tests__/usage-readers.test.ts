import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
  readClaudeUsage,
  readCodexUsage,
  readCopilotUsage,
  readGeminiUsage,
  usageReader,
} from '../usage-readers.js'

// Cut from real session files, with the prompt text and ids removed.
const fixture = (name: string) =>
  fileURLToPath(new URL(`./fixtures/usage/${name}`, import.meta.url))

describe('readClaudeUsage', () => {
  it('counts each message once, from its last line, with the sub-agents', async () => {
    expect(await readClaudeUsage(fixture('claude-session.jsonl'))).toEqual({
      'claude-opus-5-5': {
        input: 12,
        cacheRead: 3000,
        cacheWrite: 100,
        cacheWrite1h: 40,
        output: 100,
      },
      'claude-haiku-5-5': { input: 5, cacheRead: 100, cacheWrite: 0, cacheWrite1h: 0, output: 7 },
    })
  })
})

describe('readCodexUsage', () => {
  it('counts a repeated total once, and puts each change on the model of its turn', async () => {
    expect(await readCodexUsage(fixture('codex-rollout.jsonl'))).toEqual({
      'gpt-6-sol': { input: 500, cacheRead: 400, cacheWrite: 100, cacheWrite1h: 0, output: 50 },
      'gpt-6.1-sol': { input: 400, cacheRead: 1600, cacheWrite: 0, cacheWrite1h: 0, output: 100 },
    })
  })
})

describe('readCopilotUsage', () => {
  it('sums the usage records, without the cache in the fresh input', async () => {
    expect(await readCopilotUsage(fixture('copilot-events.jsonl'))).toEqual({
      'claude-sonnet-5.5': {
        input: 6,
        cacheRead: 53_900,
        cacheWrite: 12_286,
        cacheWrite1h: 0,
        output: 366,
      },
      'gpt-6.1-sol': { input: 100, cacheRead: 0, cacheWrite: 0, cacheWrite1h: 0, output: 10 },
    })
  })
})

describe('readGeminiUsage', () => {
  it('sums the tokens of each message, with the thoughts as output', async () => {
    expect(await readGeminiUsage(fixture('gemini-session.json'))).toEqual({
      'gemini-3.8-flash': {
        input: 500,
        cacheRead: 2200,
        cacheWrite: 0,
        cacheWrite1h: 0,
        output: 85,
      },
    })
  })
})

describe('usageReader', () => {
  it('has a reader for each agent that writes its usage, and none for Cursor', () => {
    expect(usageReader('claude-code')).toBe(readClaudeUsage)
    expect(usageReader('codex')).toBe(readCodexUsage)
    expect(usageReader('copilot')).toBe(readCopilotUsage)
    expect(usageReader('gemini')).toBe(readGeminiUsage)
    expect(usageReader('cursor')).toBeUndefined()
  })
})
