import type { ComponentProps } from 'react'
import { useActivity, useDiffStats } from '../../hooks/useActivity'
import type { AgentActivity } from '../../lib/api'
import { ProgressBoard } from '../stages/ProgressBoard'
import { ProgressTab } from '../stages/ProgressTab'
import { TaskBoard } from '../tasks/TaskBoard'
import { TaskDetail } from '../tasks/TaskDetail'
import { TaskTable } from '../tasks/TaskTable'
import { AgentPanes } from '../terminals/AgentPanes'

// The parts that show the activity or the sizes read them here, below the dashboard, so a change
// renders only them.
const byTask = (activity: AgentActivity) => activity.byTask
const byTerminal = (activity: AgentActivity) => activity.byTerminal

export function LiveTaskTable(
  props: Omit<ComponentProps<typeof TaskTable>, 'activity' | 'diffStats'>,
) {
  const activity = useActivity(byTask)
  const diffStats = useDiffStats()
  return <TaskTable {...props} activity={activity} diffStats={diffStats} />
}

export function LiveTaskBoard(props: Omit<ComponentProps<typeof TaskBoard>, 'diffStats'>) {
  const diffStats = useDiffStats()
  return <TaskBoard {...props} diffStats={diffStats} />
}

export function LiveTaskDetail(props: Omit<ComponentProps<typeof TaskDetail>, 'diffStats'>) {
  const diffStats = useDiffStats()
  return <TaskDetail {...props} diffStats={diffStats} />
}

export function LiveProgressBoard(props: Omit<ComponentProps<typeof ProgressBoard>, 'activity'>) {
  const activity = useActivity(byTask)
  return <ProgressBoard {...props} activity={activity} />
}

export function LiveProgressTab(props: Omit<ComponentProps<typeof ProgressTab>, 'activity'>) {
  const taskId = props.task.id
  const activity = useActivity((all) => all.byTask[taskId])
  return <ProgressTab {...props} activity={activity} />
}

export function LiveAgentPanes(props: Omit<ComponentProps<typeof AgentPanes>, 'activity'>) {
  const activity = useActivity(byTerminal)
  return <AgentPanes {...props} activity={activity} />
}
