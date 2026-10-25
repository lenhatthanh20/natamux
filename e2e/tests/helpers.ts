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
