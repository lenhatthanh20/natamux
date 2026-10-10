import { describe, expect, it } from 'vitest'

// The source of the app, without tests. Comments are left out: they may name the code's own words.
const SOURCES = import.meta.glob<string>(['../**/*.{ts,tsx}', '!../**/*.test.{ts,tsx}'], {
  eager: true,
  query: '?raw',
  import: 'default',
})

function withoutComments(text: string): string[] {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('\n')
    .filter((line) => !line.trim().startsWith('//'))
}

// Where each pattern shows up, as "file: line".
function findAll(pattern: RegExp): string[] {
  return Object.entries(SOURCES).flatMap(([file, text]) =>
    withoutComments(text)
      .filter((line) => pattern.test(line))
      .map((line) => `${file.replace('../', '')}: ${line.trim()}`),
  )
}

describe('the words of the UI', () => {
  it('reads the source files', () => {
    expect(Object.keys(SOURCES)).toContain('../components/NewWorkspaceDialog.tsx')
  })

  it('says "Start a task", not "New workspace"', () => {
    expect(findAll(/New workspace|Add a workspace|Make one/)).toEqual([])
  })

  it('keeps "Create workspace" for a task that is already there', () => {
    const files = findAll(/Create workspace/).map((match) => match.split(':')[0])
    expect([...new Set(files)].sort()).toEqual([
      'components/NoWorkspace.tsx',
      'components/TaskDetail.tsx',
    ])
  })

  it('says "project", not "repo" or "repository"', () => {
    expect(findAll(/\bRepos\b|\bRepo:|'Repo'/)).toEqual([])
    expect(findAll(/repositor/i).filter((match) => !/git repositor/i.test(match))).toEqual([])
    const files = findAll(/repositor/i).map((match) => match.split(':')[0])
    expect([...new Set(files)]).toEqual(['components/RepoPanel.tsx'])
  })
})
