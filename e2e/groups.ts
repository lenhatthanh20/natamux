// CI runs the spec files in groups, one job and one daemon each, so the slow terminal files run
// side by side. A spec file in no group here runs in 'rest'. Each job uploads the
// terminal-measures.json of its own spec files as e2e-measures-<group>.
export const GROUPS: Record<string, string[]> = {
  timing: ['dashboard-measures.spec.ts', 'terminal-measures.spec.ts'],
  cache: ['terminal-cache.spec.ts'],
  terminal: [
    'terminal-gpu.spec.ts',
    'terminal-layout.spec.ts',
    'terminal-look.spec.ts',
    'terminal-protocol.spec.ts',
  ],
}

// The spec files that a group does not run. No group (local runs) skips none.
export function skippedIn(group: string | undefined, specs: string[]): string[] {
  if (group === undefined || group === '') {
    return []
  }
  const named = Object.values(GROUPS).flat()
  if (group === 'rest') {
    return specs.filter((file) => named.includes(file))
  }
  const files = GROUPS[group]
  if (files === undefined) {
    const known = ['rest', ...Object.keys(GROUPS)].join(', ')
    throw new Error(`Unknown E2E_GROUP "${group}". Use one of: ${known}.`)
  }
  return specs.filter((file) => !files.includes(file))
}
