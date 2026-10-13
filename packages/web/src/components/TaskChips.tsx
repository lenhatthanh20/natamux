import type { Task } from '@natamux/core'

export const LABEL_CHIP =
  'inline-flex h-5 shrink-0 items-center rounded-sm border border-line px-1.5 text-xs whitespace-nowrap text-ink-2'

// The small tags after a task's title: what it waits for, what it is part of, its repository, and
// a warning when its agent went quiet. They never wrap.
export function TaskChips({
  task,
  waits,
  showRepo,
  quiet,
}: {
  task: Task
  // The ids of the tasks this one still waits for, such as "T-0003, T-0004". Empty: none.
  waits: string
  showRepo: boolean
  quiet: boolean
}) {
  return (
    <>
      {quiet && (
        <span className={`${LABEL_CHIP} border-signal-line bg-signal-soft text-signal`}>quiet</span>
      )}
      {waits !== '' && <span className={LABEL_CHIP}>waits for {waits}</span>}
      {task.parentId !== null && <span className={LABEL_CHIP}>part of {task.parentId}</span>}
      {showRepo && task.repo !== null && (
        <span className={`${LABEL_CHIP} font-mono`}>{task.repo}</span>
      )}
    </>
  )
}
