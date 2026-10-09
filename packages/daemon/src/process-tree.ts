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
