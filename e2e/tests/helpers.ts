import { type APIRequestContext, expect, type Locator, type Page } from '@playwright/test'

export const TOKEN = process.env.NATAMUX_TOKEN ?? 'e2e-token'
export const headers = { authorization: `Bearer ${TOKEN}` }

// The demo data of scripts/e2e.sh.
export const DEMO_TITLE = 'Demo: changes and history'
export const AGENT_TITLE = 'Command palette opened with Cmd+K'
export const OTHER_TITLES = [
  'Fix token refresh race in the daemon',
  'Windows path tests for repos',
  'Desktop window layout',
]

function escaped(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

// A task's row or card is named "<title>, <state>, <id>, <age>".
export function taskNamed(title: string): RegExp {
  return new RegExp(`^${escaped(title)},`)
}

// The dashboard keeps the token in localStorage, so a test can skip the sign-in screen.
export async function signIn(page: Page): Promise<void> {
  await page.addInitScript((token) => {
    localStorage.setItem('natamux.auth', token)
  }, TOKEN)
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
}

export function taskTable(page: Page): Locator {
  return page.getByRole('table', { name: 'Tasks' })
}

// Opens the task's workspace, as a double click on its row does.
export async function openTask(page: Page, title: string): Promise<Locator> {
  await page.getByRole('radio', { name: 'Table' }).click()
  await taskTable(page)
    .getByRole('button', { name: taskNamed(title) })
    .dblclick()
  const tabs = page.getByRole('tablist', { name: 'View', exact: true })
  await expect(tabs).toBeVisible()
  return tabs
}

export type Repo = { id: string; name: string }
export type TaskEvent = { type: string; payload: Record<string, unknown> }

export async function firstRepo(request: APIRequestContext): Promise<Repo> {
  const body = await (await request.get('/api/repos', { headers })).json()
  return body.repos[0]
}

export async function taskOf(request: APIRequestContext, id: string) {
  return (await (await request.get(`/api/tasks/${id}`, { headers })).json()).task
}

// Makes a task of the project. The id goes into `made`, so the test can remove the task after.
export async function newTask(
  request: APIRequestContext,
  repo: Repo,
  title: string,
  made: string[],
) {
  const created = await request.post('/api/tasks', { headers, data: { title, repo: repo.id } })
  const { task } = await created.json()
  made.push(task.id)
  return task
}

export async function openSettings(page: Page, repo: Repo) {
  await page
    .getByRole('navigation', { name: 'Workspaces' })
    .getByRole('button', { name: 'Add a project' })
    .click()
  await page
    .getByRole('dialog', { name: 'Projects' })
    .getByRole('listitem')
    .filter({ hasText: repo.name })
    .getByRole('button', { name: 'Settings' })
    .click()
  const dialog = page.getByRole('dialog', { name: `Project settings of ${repo.name}` })
  await expect(dialog.getByRole('tab', { name: 'Stages', selected: true })).toBeVisible()
  return dialog
}

// The default pipeline before the simple one (T-0282), as the catalog sets its stages. Tests that
// need Plan, the Test spec and Implement group, or Automated tests save it first.
export const FULL_PIPELINE = [
  { id: 'context', kind: 'context', gate: 'manual', output: 'context.md' },
  {
    id: 'spec',
    kind: 'agent',
    gate: 'manual',
    output: 'spec.md',
    sections: ['Goal', 'Scope', 'Out of scope', 'Acceptance criteria', 'Code map'],
  },
  {
    id: 'plan',
    kind: 'agent',
    gate: 'manual',
    output: 'plan.md',
    sections: ['Steps', 'Files', 'Tests'],
    workflow: true,
  },
  {
    id: 'build',
    name: 'Test spec and Implement',
    parallel: [
      {
        id: 'test-spec',
        kind: 'agent',
        gate: 'manual',
        output: 'test-spec.md',
        sections: ['Cases'],
        ownShell: true,
      },
      { id: 'implement', kind: 'agent', gate: 'auto', workflow: true, ci: true },
    ],
  },
  { id: 'testing', name: 'Automated tests', kind: 'testing', gate: 'auto', ci: true },
  {
    id: 'agent-review',
    name: 'Review',
    kind: 'agent',
    gate: 'auto',
    output: 'review.md',
    sections: ['Findings'],
    ownShell: true,
  },
  { id: 'release', kind: 'release', gate: 'manual' },
]

// Saves FULL_PIPELINE as the stages of the project. The promise gives back the settings to put
// back after the test, since the tests share one daemon.
export async function useFullPipeline(
  request: APIRequestContext,
  repo: Repo,
): Promise<{ text: string | null }> {
  const url = `/api/repos/${repo.id}/settings`
  const before = await (await request.get(url, { headers })).json()
  const settings = before.text === null ? {} : JSON.parse(before.text)
  const saved = await request.put(url, { headers, data: { ...settings, stages: FULL_PIPELINE } })
  if (!saved.ok()) {
    throw new Error(`Saving the full pipeline failed: ${saved.status()} ${await saved.text()}`)
  }
  return before
}

// Puts back the settings that useFullPipeline gave.
export async function restoreSettings(
  request: APIRequestContext,
  repo: Repo,
  before: { text: string | null },
): Promise<void> {
  await request.put(`/api/repos/${repo.id}/settings`, {
    headers,
    data: before.text === null ? {} : JSON.parse(before.text),
  })
}
