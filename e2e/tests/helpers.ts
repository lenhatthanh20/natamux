import { expect, type Locator, type Page } from '@playwright/test'

export const TOKEN = process.env.NATAMUX_TOKEN ?? 'e2e-token'

// The demo data of scripts/e2e.sh.
export const DEMO_TITLE = 'Demo: changes and history'
export const AGENT_TITLE = 'Command palette opened with Cmd+K'
export const OTHER_TITLES = [
  'Fix token refresh race in the daemon',
  'Windows path tests for repos',
  'Electron shell spike',
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
