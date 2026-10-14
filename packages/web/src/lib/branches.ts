import type { ComboboxOption } from '../components/ui/Combobox'

// The branches that Natamux made for tasks. They fill the list, so they are folded away.
export const TASK_BRANCH_GROUP = 'Task branches'
const TASK_BRANCH_PREFIX = 'natamux/'

// The list of the "Branch from" picker: the default branch, the branch of the checkout and the
// one used last come first, then the other branches, then the task branches.
export function branchOptions(
  branches: readonly string[],
  current: string | null,
  lastUsed: string | null,
  defaultBranch: string | null = null,
): ComboboxOption[] {
  const recent = [defaultBranch, current, lastUsed]
    .filter((name): name is string => name !== null && branches.includes(name))
    .filter((name, index, all) => all.indexOf(name) === index)
  const rest = branches.filter((name) => !recent.includes(name))
  return [
    ...recent.map((value) => ({
      value,
      label: labelOf(value, value === defaultBranch, value === current),
      group: 'Recent',
    })),
    ...rest
      .filter((name) => !name.startsWith(TASK_BRANCH_PREFIX))
      .map((value) => ({ value, label: value, group: 'Branches' })),
    ...rest
      .filter((name) => name.startsWith(TASK_BRANCH_PREFIX))
      .map((value) => ({ value, label: value, group: TASK_BRANCH_GROUP })),
  ]
}

function labelOf(name: string, isDefault: boolean, isCurrent: boolean): string {
  const marks = [isDefault ? 'default' : null, isCurrent ? 'current' : null].filter(Boolean)
  return marks.length === 0 ? name : `${name} (${marks.join(', ')})`
}

const LAST_BASE_KEY = 'natamux.last-base'

function savedBases(): Record<string, string> {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(LAST_BASE_KEY) ?? '{}')
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      return {}
    }
    return Object.fromEntries(
      Object.entries(value).filter(
        (entry): entry is [string, string] => typeof entry[1] === 'string',
      ),
    )
  } catch {
    return {}
  }
}

// The branch that a workspace of this repository started from the last time.
export function lastBase(repo: string): string | null {
  return savedBases()[repo] ?? null
}

export function rememberBase(repo: string, branch: string): void {
  try {
    globalThis.localStorage?.setItem(
      LAST_BASE_KEY,
      JSON.stringify({ ...savedBases(), [repo]: branch }),
    )
  } catch {
    // Not kept; the picker starts with the current branch again.
  }
}
