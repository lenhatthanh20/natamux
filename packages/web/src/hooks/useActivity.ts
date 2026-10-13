import type { DiffStat } from '@natamux/core'
import { hashKey, type QueryKey, useQueryClient } from '@tanstack/react-query'
import { useCallback, useSyncExternalStore } from 'react'
import type { AgentActivity } from '../lib/api'
import { ACTIVITY_KEY, DIFFSTATS_KEY } from '../lib/live'

// Readers of data the dashboard loads and the live feed keeps current. Each reader takes only its
// part, so a change renders only the parts it changes. They read the cache with no query observer
// of their own: they never load, so a hundred rows do not mean a hundred requests, and a reader
// without the dashboard (a test) does not make the query run with no queryFn.
function useCached<T, R>(key: QueryKey, select: (data: T) => R): R | undefined {
  const client = useQueryClient()
  const hash = hashKey(key)
  const subscribe = useCallback(
    (onChange: () => void) =>
      client.getQueryCache().subscribe((event) => {
        if (event.query.queryHash === hash) {
          onChange()
        }
      }),
    [client, hash],
  )
  // The cache shares the parts that did not change, so a part that a select returns stays the
  // same object until it changes.
  return useSyncExternalStore(subscribe, () => {
    const data = client.getQueryData<T>(key)
    return data === undefined ? undefined : select(data)
  })
}

export function useActivity<T>(select: (activity: AgentActivity) => T): T | undefined {
  return useCached(ACTIVITY_KEY, select)
}

const whole = (stats: Record<string, DiffStat>) => stats

export function useDiffStats(): Record<string, DiffStat> | undefined {
  return useCached(DIFFSTATS_KEY, whole)
}
