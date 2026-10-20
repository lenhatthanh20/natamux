import type { TaskUsage, UsageRow } from '@natamux/core'

const TOKENS = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 })
const USD = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' })

// 950, 12.3K, 610K, 1.2M.
export function formatTokens(count: number): string {
  return TOKENS.format(count)
}

// $0.84, <$0.01, or — when there is no price.
export function formatUsd(cost: number | null): string {
  if (cost === null) {
    return '—'
  }
  return cost > 0 && cost < 0.005 ? '<$0.01' : USD.format(cost)
}

export function rowTokens(row: UsageRow): number {
  return row.input + row.cacheRead + row.cacheWrite + row.output
}

// "$1.84 est. · 2.4M tokens", or the tokens alone when no model has a price.
export function usageLine(tokens: number, costUsd: number | null): string {
  const count = `${formatTokens(tokens)} tokens`
  return costUsd === null ? count : `${formatUsd(costUsd)} est. · ${count}`
}

// The stage's part of the task's usage. Undefined when the stage used nothing.
export function stageUsage(
  usage: TaskUsage | undefined,
  stage: string,
): { tokens: number; costUsd: number | null } | undefined {
  const rows = usage?.stages.filter((row) => row.stage === stage) ?? []
  if (rows.length === 0) {
    return undefined
  }
  const priced = rows.flatMap((row) => (row.costUsd === null ? [] : [row.costUsd]))
  return {
    tokens: rows.reduce((sum, row) => sum + rowTokens(row), 0),
    costUsd: priced.length === 0 ? null : priced.reduce((sum, cost) => sum + cost, 0),
  }
}
