// Each task gets its own block of ports, so dev servers of tasks that run side by side do not
// clash. The block comes from the task number: T-0001 gets 20010 to 20019. Numbers wrap after
// 1000 tasks.
export const PORT_BASE = 20_000
export const PORTS_PER_TASK = 10
const BLOCKS = 1000

export function portRange(taskId: string): { first: number; last: number } {
  const number = Number(/^T-(\d+)$/.exec(taskId)?.[1] ?? '0')
  const first = PORT_BASE + (number % BLOCKS) * PORTS_PER_TASK
  return { first, last: first + PORTS_PER_TASK - 1 }
}
