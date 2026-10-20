import { describe, expect, it } from 'vitest'
import { formatTokens, formatUsd, stageUsage, usageLine } from '../usage-format'

const row = (stage: string, costUsd: number | null) => ({
  stage,
  agent: 'claude-code',
  model: 'claude-opus-5-5',
  input: 100,
  cacheRead: 200,
  cacheWrite: 300,
  output: 400,
  costUsd,
})

describe('formatTokens', () => {
  it.each([
    [950, '950'],
    [12_345, '12.3K'],
    [610_000, '610K'],
    [1_234_567, '1.2M'],
  ])('writes %d as %s', (count, text) => {
    expect(formatTokens(count)).toBe(text)
  })
})

describe('formatUsd', () => {
  it.each([
    [0.84, '$0.84'],
    [1234.5, '$1,234.50'],
    [0.004, '<$0.01'],
    [0, '$0.00'],
    [null, '—'],
  ])('writes %s as %s', (cost, text) => {
    expect(formatUsd(cost)).toBe(text)
  })
})

describe('usageLine', () => {
  it('names the estimate and the tokens, or the tokens alone without a price', () => {
    expect(usageLine(2_400_000, 1.84)).toBe('$1.84 est. · 2.4M tokens')
    expect(usageLine(610_000, null)).toBe('610K tokens')
  })
})

describe('stageUsage', () => {
  it("sums the stage's rows, with the cost of the priced ones", () => {
    const usage = {
      tokens: 3000,
      costUsd: 0.5,
      unpricedModels: ['mystery'],
      stages: [row('implement', 0.25), row('implement', null), row('review', 0.25)],
    }
    expect(stageUsage(usage, 'implement')).toEqual({ tokens: 2000, costUsd: 0.25 })
    expect(stageUsage(usage, 'spec')).toBeUndefined()
    expect(stageUsage(undefined, 'implement')).toBeUndefined()
  })
})
