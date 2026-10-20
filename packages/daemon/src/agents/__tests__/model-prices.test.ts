import { describe, expect, it } from 'vitest'
import { costOf, normalizeModel, priceOf } from '../model-prices.js'

const counts = (input: number, cacheRead: number, cacheWrite: number, output: number) => ({
  input,
  cacheRead,
  cacheWrite,
  cacheWrite1h: 0,
  output,
})

describe('normalizeModel', () => {
  it.each([
    ['claude-sonnet-5.5', 'claude-sonnet-5-5'],
    ['Claude-Opus-4-6-20260205', 'claude-opus-4-6'],
    ['gemini-2.5-pro-2026-01-15', 'gemini-2-5-pro'],
    ['gpt-6.1-sol', 'gpt-6-1-sol'],
  ])('reads %s as %s', (model, id) => {
    expect(normalizeModel(model)).toBe(id)
  })
})

describe('priceOf', () => {
  it('knows the models of each agent, in the forms they write', () => {
    expect(priceOf('claude-opus-5-5')?.input).toBe(4)
    expect(priceOf('claude-sonnet-5.5')).toEqual(priceOf('claude-sonnet-5-5'))
    expect(priceOf('gpt-6.1-sol')?.cacheRead).toBe(0.1)
    expect(priceOf('gemini-3.8-flash')?.output).toBe(3.75)
  })

  it('has no price for a model not in the table', () => {
    expect(priceOf('codex-auto-review')).toBeUndefined()
    expect(priceOf('unknown')).toBeUndefined()
  })
})

describe('costOf', () => {
  it('prices each kind of token, the long cache writes at their own price', () => {
    const million = { ...counts(1e6, 1e6, 1e6, 1e6), cacheWrite1h: 400_000 }
    // 4 input + 0.2 cache read + 0.6 × 5 short writes + 0.4 × 8 long writes + 20 output.
    expect(costOf(million, 'claude-opus-5-5')).toBeCloseTo(30.4)
  })

  it('bills cache writes as input for a model without a write price', () => {
    expect(costOf(counts(0, 0, 1e6, 0), 'gemini-2.5-pro')).toBeCloseTo(1.25)
  })

  it('is null for a model without a price', () => {
    expect(costOf(counts(10, 0, 0, 10), 'codex-auto-review')).toBeNull()
  })
})
