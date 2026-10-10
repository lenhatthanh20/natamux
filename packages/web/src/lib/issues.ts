// What went wrong with GitHub, in a sentence that says how to fix it. The raw text stays
// available behind "Show details".
export function issueProblem(message: string): string {
  if (/not (installed|found)|ENOENT|command not found|no such file/i.test(message)) {
    return 'The GitHub CLI is not installed. Install `gh`, run `gh auth login` in a terminal, then try again.'
  }
  if (/auth|log ?in|token|credential|403|401/i.test(message)) {
    return 'GitHub CLI is not signed in. Run `gh auth login` in a terminal, then try again.'
  }
  return 'Natamux could not read the issues from GitHub. Check the connection and try again.'
}

export function importLabel(count: number): string {
  if (count === 0) {
    return 'Import'
  }
  return count === 1 ? 'Import 1 issue' : `Import ${count} issues`
}

// A repository folder must be a full path. The daemon checks the rest.
export function repoPathProblem(path: string): string | null {
  const value = path.trim()
  if (value === '' || /^([/~]|[A-Za-z]:[\\/]|\\\\)/.test(value)) {
    return null
  }
  return 'Use the full path of the folder, starting with / (or a drive letter on Windows).'
}
