// The terminals to show as tabs: the first one always, the open ones, and the ones just added.
export function terminalNames(open: readonly string[], added: readonly string[]): string[] {
  return [...new Set(['1', ...open, ...added])].sort((a, b) => Number(a) - Number(b))
}

export function nextTerminalName(names: readonly string[]): string {
  return String(Math.max(0, ...names.map(Number)) + 1)
}

// The agent shells that the list of open terminals does not have: the daemon opened them after
// the list was read, as it does for each stage of a group that starts.
export function unlistedShells(open: readonly string[], agentShells: readonly string[]): string[] {
  return agentShells.filter((name) => !open.includes(name))
}
