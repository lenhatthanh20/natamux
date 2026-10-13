export type Mark = [name: string, at: number]

// One line for the log, so a start writes one line, not one per mark.
export function startupLine(marks: Mark[]): string {
  const parts = marks.map(([name, at]) => `${name} ${at}`)
  return `Startup, ms since the process began: ${parts.join(', ')}`
}
