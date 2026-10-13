import type { DiffStat } from '@natamux/core'
import { useQuery } from '@tanstack/react-query'
import type { AgentActivity } from '../lib/api'
import { ACTIVITY_KEY, DIFFSTATS_KEY } from '../lib/live'

// Readers of data the dashboard loads and the live feed keeps current. Each reader takes only its
// part, so a change renders only the parts it changes. They never load, so a hundred rows do not
// mean a hundred requests: the dashboard owns each query and its poll.
export function useActivity<T>(select: (activity: AgentActivity) => T): T | undefined {
  const { data } = useQuery<AgentActivity, Error, T>({
    queryKey: ACTIVITY_KEY,
    enabled: false,
    select,
  })
  return data
}

export function useDiffStats(): Record<string, DiffStat> | undefined {
  return useQuery<Record<string, DiffStat>>({ queryKey: DIFFSTATS_KEY, enabled: false }).data
}
