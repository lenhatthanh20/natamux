import type { UsageCounts } from './usage-readers.js'

// USD per million tokens. `cacheWrite` is the short cache (Claude 5 minutes, OpenAI 30 minutes),
// `cacheWrite1h` the long one (Claude 1 hour, OpenAI 12 hours).
type Price = {
  input: number
  cacheRead: number
  cacheWrite: number
  cacheWrite1h: number
  output: number
}

const price = (
  input: number,
  cacheRead: number,
  cacheWrite: number,
  cacheWrite1h: number,
  output: number,
): Price => ({ input, cacheRead, cacheWrite, cacheWrite1h, output })

// A model without a cache write price bills cache writes as input.
const noWrite = (input: number, cacheRead: number, output: number): Price =>
  price(input, cacheRead, input, input, output)

// API list prices, standard tier, short prompts, read 2026-10-21 from the claude-api skill
// (Anthropic), developers.openai.com/api/docs/pricing and ai.google.dev/gemini-api/docs/pricing.
// Keys are model ids in the form of normalizeModel.
const PRICES: Record<string, Price> = {
  'claude-fable-5-1': price(10, 0.25, 12.5, 20, 50),
  'claude-mythos-5-1': price(10, 0.25, 12.5, 20, 50),
  'claude-fable-5': price(10, 1, 12.5, 20, 50),
  'claude-mythos-5': price(10, 1, 12.5, 20, 50),
  'claude-opus-5-5': price(4, 0.2, 5, 8, 20),
  'claude-opus-5': price(5, 0.5, 6.25, 10, 25),
  'claude-opus-4-8': price(5, 0.5, 6.25, 10, 25),
  'claude-opus-4-7': price(5, 0.5, 6.25, 10, 25),
  'claude-opus-4-6': price(5, 0.5, 6.25, 10, 25),
  'claude-sonnet-5-5': price(2, 0.2, 2.5, 4, 10),
  'claude-sonnet-5': price(2, 0.2, 2.5, 4, 10),
  'claude-sonnet-4-6': price(3, 0.3, 3.75, 6, 15),
  // Prompts of 100K tokens or fewer; longer ones cost 5 times more.
  'claude-haiku-5-5': price(0.1, 0.01, 0.125, 0.2, 0.5),
  'claude-haiku-4-5': price(1, 0.1, 1.25, 2, 5),

  'gpt-6-astra': price(10, 1, 12.5, 25, 50),
  'gpt-6-1-sol': price(2, 0.1, 2.5, 5, 10),
  'gpt-6-sol': price(2, 0.2, 2.5, 5, 10),
  'gpt-6-luna': price(0.1, 0.01, 0.125, 0.25, 0.5),
  'gpt-5-6-sol': price(4, 0.4, 5, 10, 20),
  'gpt-5-6-terra': price(2, 0.2, 2.5, 5, 12),
  'gpt-5-6-luna': price(0.2, 0.02, 0.25, 0.5, 1.2),
  'gpt-5-5': noWrite(5, 0.5, 30),
  'gpt-5-4': noWrite(2.5, 0.25, 15),
  'gpt-5-4-mini': noWrite(0.75, 0.075, 4.5),
  'gpt-5-4-nano': noWrite(0.2, 0.02, 1.25),
  'gpt-5-3-codex': noWrite(1.75, 0.175, 14),
  'gpt-5-2': noWrite(1.75, 0.175, 14),
  'gpt-5-1': noWrite(1.25, 0.125, 10),
  'gpt-5': noWrite(1.25, 0.125, 10),
  'gpt-5-mini': noWrite(0.25, 0.025, 2),
  'gpt-5-nano': noWrite(0.05, 0.005, 0.4),

  'gemini-4-argon': noWrite(2, 0.1, 10),
  'gemini-3-8-flash': noWrite(0.75, 0.075, 3.75),
  'gemini-3-6-flash': noWrite(0.75, 0.075, 3.75),
  'gemini-3-5-flash-lite': noWrite(0.3, 0.03, 2.5),
  'gemini-3-1-flash-lite': noWrite(0.25, 0.025, 1.5),
  // Prompts of 200K tokens or fewer, as the two below.
  'gemini-3-1-pro-preview': noWrite(2, 0.2, 12),
  'gemini-3-flash-preview': noWrite(0.5, 0.05, 3),
  'gemini-2-5-pro': noWrite(1.25, 0.125, 10),
  'gemini-2-5-flash': noWrite(0.3, 0.03, 2.5),
  'gemini-2-5-flash-lite': noWrite(0.1, 0.01, 0.4),
}

// Copilot writes "claude-sonnet-5.5", others add a date such as "-20260401" or "-2026-04-01".
export function normalizeModel(model: string): string {
  return model
    .trim()
    .toLowerCase()
    .replace(/-(\d{8}|\d{4}-\d{2}-\d{2})$/, '')
    .replaceAll('.', '-')
}

export function priceOf(model: string): Price | undefined {
  return PRICES[normalizeModel(model)]
}

// Null when the model has no price.
export function costOf(counts: UsageCounts, model: string): number | null {
  const per = priceOf(model)
  if (per === undefined) {
    return null
  }
  const cacheWrite5m = Math.max(0, counts.cacheWrite - counts.cacheWrite1h)
  return (
    (counts.input * per.input +
      counts.cacheRead * per.cacheRead +
      cacheWrite5m * per.cacheWrite +
      counts.cacheWrite1h * per.cacheWrite1h +
      counts.output * per.output) /
    1_000_000
  )
}
