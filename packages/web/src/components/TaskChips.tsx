import { type Task, waitingOn } from '@natamux/core'

export const LABEL_CHIP =
  'inline-flex h-5 shrink-0 items-center rounded-sm border border-line px-1.5 text-xs whitespace-nowrap text-ink-2'

// The small tags after a task's title: what it waits for, what it is part of, its repository, and
// a warning when its agent went quiet. They never wrap.
export function TaskChips({
  task,
  allTasks,
  showRepo,
  quiet,
}: {
  task: Task
  // Every task, to tell whether the ones this task waits for are done.
  allTasks: readonly Task[]
  showRepo: boolean
  quiet: boolean
}) {
  const waits = waitingOn(task, allTasks)
  return (
    <>
      {quiet && (
        <span className={`${LABEL_CHIP} border-signal-line bg-signal-soft text-signal`}>quiet</span>
      )}
      {waits.length > 0 && <span className={LABEL_CHIP}>waits for {waits.join(', ')}</span>}
      {task.parentId !== null && <span className={LABEL_CHIP}>part of {task.parentId}</span>}
      {showRepo && task.repo !== null && (
        <span className={`${LABEL_CHIP} font-mono`}>{task.repo}</span>
      )}
    </>
  )
}
