// The processes started under `root`, children first: the deepest come last in the list, so
// they are signalled first. Reads `ps -A -o pid=,ppid=` output.
export function descendants(psOutput: string, root: number): number[] {
  const children = new Map<number, number[]>()
  for (const line of psOutput.split('\n')) {
    const [pid, ppid] = line.trim().split(/\s+/).map(Number)
    if (
      pid !== undefined &&
      ppid !== undefined &&
      Number.isInteger(pid) &&
      Number.isInteger(ppid)
    ) {
      children.set(ppid, [...(children.get(ppid) ?? []), pid])
    }
  }
  const found: number[] = []
  const visit = (pid: number) => {
    for (const child of children.get(pid) ?? []) {
      found.push(child)
      visit(child)
    }
  }
  visit(root)
  return found
}

// The command lines of the processes under `root`, parents first. Reads
// `ps -A -o pid=,ppid=,args=` output.
export function commandsUnder(psOutput: string, root: number): string[] {
  const args = new Map<number, string>()
  for (const line of psOutput.split('\n')) {
    const match = /^\s*(\d+)\s+\d+\s+(.*)$/.exec(line)
    if (match?.[1] !== undefined && match[2] !== undefined) {
      args.set(Number(match[1]), match[2])
    }
  }
  return descendants(psOutput, root).flatMap((pid) => args.get(pid) ?? [])
}

// The ones of `pids` that still run. A zombie has ended: it only waits for its parent to read
// its exit. Reads `ps -A -o pid=,ppid=,stat=` output.
export function stillRunning(psOutput: string, pids: number[]): number[] {
  const wanted = new Set(pids)
  return psOutput.split('\n').flatMap((line) => {
    const [pid, , stat] = line.trim().split(/\s+/)
    return wanted.has(Number(pid)) && stat !== undefined && !stat.startsWith('Z')
      ? [Number(pid)]
      : []
  })
}
