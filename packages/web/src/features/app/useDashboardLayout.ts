import type { DiffStat, Task } from '@natamux/core'
import { useQueries } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useStoredWidth } from '../../components/ui/ResizeHandle'
import { useElementWidth } from '../../hooks/useElementWidth'
import { useFrozenOrder } from '../../hooks/useFrozenOrder'
import { useLayoutMode, useViewportWidth } from '../../hooks/useLayoutMode'
import { clockNow, useNowValue } from '../../hooks/useNow'
import type { Api } from '../../lib/api'
import { effectiveSidebarWidth, PANEL_WIDTH } from '../../lib/layout-mode'
import { agentPaneTasks } from '../../lib/panes'
import { agentLanes, restoringShells, taskShellsQuery } from '../../lib/task-shells'
import { filterTasks, isQuiet, needsMeTasks, sortTasks } from '../../lib/tasks'
import { detailsOpenFor, type useViewStore } from '../../lib/view-store'
import { triesOf, tryGroup } from '../tasks/tries'

type View = ReturnType<typeof useViewStore.getState>

type LayoutInput = Pick<
  View,
  | 'stateFilter'
  | 'search'
  | 'selectedId'
  | 'detailOpenByTask'
  | 'sidebarOpen'
  | 'changesTaskId'
  | 'agentsOpen'
  | 'agentPanes'
  | 'sort'
> & {
  api: Api
  tasks: Task[]
  diffStats: Record<string, DiffStat> | undefined
}

// What the dashboard shows and where: the rows in their order, the open task, and the widths of
// the sidebar, the details and the agent panes.
export function useDashboardLayout({
  api,
  tasks,
  diffStats,
  stateFilter,
  search,
  selectedId,
  detailOpenByTask,
  sidebarOpen,
  changesTaskId,
  agentsOpen,
  agentPanes,
  sort,
}: LayoutInput) {
  // The search narrows the counts of the state filter, and the filter narrows the rows.
  const searchedTasks = useMemo(() => filterTasks(tasks, null, search), [tasks, search])
  const visibleTasks = useMemo(
    () => filterTasks(tasks, stateFilter, search),
    [tasks, stateFilter, search],
  )
  // One stable order, which holds still under the pointer and right after a key moved the selection.
  const sortedTasks = useMemo(
    () => sortTasks(visibleTasks, sort, diffStats),
    [visibleTasks, sort, diffStats],
  )
  const frozen = useFrozenOrder(
    sortedTasks,
    `${sort?.key}:${sort?.direction}|${stateFilter}|${search}`,
  )
  const orderedTasks = frozen.items
  // The view header is as wide as the main area, so its width picks the table columns.
  const [headerRef, mainWidth] = useElementWidth<HTMLDivElement>()
  const selectedTask = tasks.find((task) => task.id === selectedId) ?? null
  const changesTask = tasks.find((task) => task.id === changesTaskId) ?? null
  // The open workspace's task, or else the task chosen in the list.
  const detailTask = changesTask ?? selectedTask
  // Beside a workspace the user watches the agent, so the details show its progress first.
  const besideWorkspace = changesTask !== null && changesTask.workspace !== null
  const detailGroup = detailTask === null ? null : tryGroup(detailTask)
  const detailTries = detailGroup === null ? [] : triesOf(detailGroup, tasks)
  // A running task turns quiet as time passes. The clock is read as the ids of the quiet tasks, so
  // it re-renders the dashboard only when one turns quiet, not on every tick.
  const quietIds = useNowValue((now) =>
    tasks
      .filter((task) => isQuiet(task, now))
      .map((task) => task.id)
      .join(' '),
  )
  // The tasks that need the user ignore the search: they must stay in sight.
  // biome-ignore lint/correctness/useExhaustiveDependencies: quietIds is the trigger of the clock.
  const waiting = useMemo(() => needsMeTasks(tasks, clockNow()), [tasks, quietIds])
  const sidebar = useStoredWidth('natamux.sidebar-width', PANEL_WIDTH, 220, 440)
  const details = useStoredWidth('natamux.details-width', PANEL_WIDTH, 320, 560)
  const viewportWidth = useViewportWidth()
  const sidebarWidth = effectiveSidebarWidth(sidebar.width, viewportWidth)
  const shellLayout = useLayoutMode({
    sidebarWidth: sidebarOpen ? sidebarWidth : 0,
    detailsWidth: details.width,
  })
  const phone = shellLayout.details === 'page'
  // The stage of each agent shell in the agents grid, so each pane shows its own state. An agent
  // starts and ends in its shell without a task event; the daemon says so with shells.changed.
  const paneTasks = agentsOpen ? agentPaneTasks(agentPanes, tasks) : []
  const paneQueries = useQueries({
    queries: paneTasks.map((task) => taskShellsQuery(task.id, (id) => api.listShells(id))),
  })
  const paneTerminals = Object.fromEntries(
    paneTasks.flatMap((task, index) => {
      const shells = paneQueries[index]?.data
      return shells === undefined ? [] : [[task.id, agentLanes(shells)]]
    }),
  )
  // A pane of the grid is Shell 1, which may be coming back after a restart.
  const paneRestoring = Object.fromEntries(
    paneTasks.flatMap((task, index) => {
      const shells = paneQueries[index]?.data
      const agent = shells === undefined ? undefined : restoringShells(shells)['1']
      return agent === undefined ? [] : [[task.id, agent]]
    }),
  )
  const dockedBesideWorkspace = besideWorkspace && shellLayout.details === 'docked'
  const detailsOpenOf = (id: string) =>
    detailsOpenFor(detailOpenByTask[id], dockedBesideWorkspace && id === changesTask?.id)
  const detailOpen = detailTask !== null && !agentsOpen && detailsOpenOf(detailTask.id)

  return {
    searchedTasks,
    visibleTasks,
    frozen,
    orderedTasks,
    headerRef,
    mainWidth,
    selectedTask,
    changesTask,
    detailTask,
    besideWorkspace,
    detailTries,
    waiting,
    sidebar,
    details,
    viewportWidth,
    sidebarWidth,
    shellLayout,
    phone,
    paneTerminals,
    paneRestoring,
    dockedBesideWorkspace,
    detailsOpenOf,
    detailOpen,
  }
}
