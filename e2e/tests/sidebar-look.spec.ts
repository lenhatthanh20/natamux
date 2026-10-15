import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test'
import { openTask, signIn, TOKEN, taskNamed } from './helpers'
import { pane, removeTasks, rows, runIn } from './terminal-helpers'

// Test spec of T-0023: the agent icon on line 2, the project header, and the ring of an agent shell
// whose state is not known.

const headers = { authorization: `Bearer ${TOKEN}` }
const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

type Repo = { id: string; name: string }

async function firstRepo(request: APIRequestContext): Promise<Repo> {
  return (await (await request.get('/api/repos', { headers })).json()).repos[0]
}

// A task of Claude Code with a workspace, held at its first stage so no agent starts in shell 1.
async function heldTask(request: APIRequestContext, title: string): Promise<string> {
  const repo = await firstRepo(request)
  const created = await request.post('/api/tasks', {
    headers,
    data: { title, repo: repo.id, agent: 'claude-code' },
  })
  const { task } = await created.json()
  made.push(task.id)
  const stateOf = async () =>
    (await (await request.get(`/api/tasks/${task.id}`, { headers })).json()).task.state
  await expect.poll(stateOf, { timeout: 30_000 }).toMatch(/^(ready|review)$/)
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'context' },
    },
  })
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })
  return task.id
}

function tree(page: Page): Locator {
  return page
    .getByRole('navigation', { name: 'Workspaces' })
    .getByRole('region', { name: 'Workspaces' })
}

async function setTheme(page: Page, theme: 'dark' | 'light'): Promise<void> {
  await page.evaluate((value) => {
    document.documentElement.dataset.theme = value
  }, theme)
}

// The contrast of the text of an element on what is drawn under it: each background from the page
// down to the element, laid one on the other. Computed colors come as rgb() or color(srgb …).
async function contrastOf(target: Locator): Promise<number> {
  return target.evaluate((element) => {
    type Rgba = [number, number, number, number]
    const parse = (value: string): Rgba => {
      const srgb = value.match(/color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)/)
      if (srgb) {
        const [r, g, b] = [srgb[1], srgb[2], srgb[3]].map((part) => Number(part) * 255)
        return [r ?? 0, g ?? 0, b ?? 0, srgb[4] === undefined ? 1 : Number(srgb[4])]
      }
      const parts = (value.match(/[\d.]+/g) ?? []).map(Number)
      return [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0, parts[3] ?? 1]
    }
    const over = (top: Rgba, under: Rgba): Rgba => {
      const a = top[3]
      return [
        top[0] * a + under[0] * (1 - a),
        top[1] * a + under[1] * (1 - a),
        top[2] * a + under[2] * (1 - a),
        1,
      ]
    }
    const layers: Element[] = []
    for (let at: Element | null = element; at !== null; at = at.parentElement) {
      layers.unshift(at)
    }
    let ground: Rgba = [255, 255, 255, 1]
    for (const layer of layers) {
      ground = over(parse(getComputedStyle(layer).backgroundColor), ground)
    }
    const text = over(parse(getComputedStyle(element).color), ground)
    const luminance = ([r, g, b]: Rgba) => {
      const [lr, lg, lb] = [r, g, b].map((channel) => {
        const c = channel / 255
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
      })
      return 0.2126 * (lr ?? 0) + 0.7152 * (lg ?? 0) + 0.0722 * (lb ?? 0)
    }
    const [light, dark] = [luminance(text), luminance(ground)].sort((a, b) => b - a)
    return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05)
  })
}

// C1, C3, C7–C11: line 2 shows only the agent icon, folded and open; the project header is a band
// with a strong name and a count that reads well in both themes.
test('shows the agent icon and the project band', async ({ page, request }, testInfo) => {
  const title = `Sidebar look ${Date.now()}`
  await heldTask(request, title)
  const repo = await firstRepo(request)
  await page.reload()

  const row = tree(page).getByRole('button', { name: taskNamed(title) })
  await expect(row).toBeVisible()
  const agent = row.locator('[data-tip="Claude Code"]')
  await expect(agent.locator('img')).toBeVisible()
  await expect(row.getByText('Claude Code', { exact: true })).toHaveCount(0)

  // The header names the project and how many workspaces it has.
  const header = tree(page).getByRole('button', { name: new RegExp(`^${repo.name}`) })
  const count = header.getByRole('img', { name: /^\d+ workspaces?$/ })
  await expect(count).toBeVisible()
  const band = header.locator('xpath=..')
  // 2px between the band and the first row.
  const bandBox = await band.boundingBox()
  const firstRow = await band.locator('xpath=following-sibling::ul[1]/li[1]').boundingBox()
  expect(bandBox).not.toBeNull()
  expect(firstRow).not.toBeNull()
  if (bandBox !== null && firstRow !== null) {
    expect(bandBox.height).toBeCloseTo(28, 0)
    expect(Math.round(firstRow.y - (bandBox.y + bandBox.height))).toBe(2)
  }

  for (const theme of ['light', 'dark'] as const) {
    await setTheme(page, theme)
    await page.mouse.move(0, 0)
    expect(await contrastOf(count), `count on the band, ${theme}`).toBeGreaterThanOrEqual(4.5)
    await header.hover()
    expect(await contrastOf(count), `count on the hovered band, ${theme}`).toBeGreaterThanOrEqual(
      4.5,
    )
    await testInfo.attach(`sidebar-look-${theme}.png`, {
      body: await page.getByRole('navigation', { name: 'Workspaces' }).screenshot(),
      contentType: 'image/png',
    })
  }

  // Open, the row keeps the icon.
  await row.click()
  await expect(row).toHaveAttribute('aria-current', 'true')
  await expect(agent.locator('img')).toBeVisible()
})

// C13, C15, C16: an agent started by hand in Shell 2 has no state until its first hook. The row
// shows a ring, then the state the hooks give, and the daemon keeps it while no restart comes.
test('shows a ring, then the state the hooks give', async ({ page, request }, testInfo) => {
  const title = `Shell ring ${Date.now()}`
  const id = await heldTask(request, title)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await page.getByRole('button', { name: 'New shell' }).click()
  const shell = pane(page, 'Shell 2')
  await expect(rows(shell)).toContainText('$', { timeout: 30_000 })
  // Any program that is no shell counts as an agent there.
  await runIn(page, shell, 'sleep 300')

  const lane = async () => {
    const response = await request.get(`/api/tasks/${id}/terminals`, { headers })
    return (await response.json()).lanes?.['2'] ?? 'none'
  }
  await expect.poll(lane, { timeout: 15_000 }).toEqual({ stage: null, state: null, agent: null })

  const row = tree(page).getByRole('button', { name: taskNamed(title) })
  const item = row.locator('xpath=ancestor::li[1]')
  const unknown = page.getByRole('button', { name: 'Shell 2, State not known', exact: true })
  // Once the shell's output goes quiet, the daemon sees the new program and sends shells.changed.
  await expect(unknown).toBeVisible({ timeout: 15_000 })
  await expect(unknown.locator('[data-tip="State not known yet"] svg')).toBeVisible()
  await testInfo.attach('shell-ring.png', {
    body: await item.screenshot(),
    contentType: 'image/png',
  })

  // A hook from Shell 2, as Claude Code sends it: the prompt, then the end of the turn.
  const hook = (name: string) =>
    request.post('/api/hooks/claude-code', {
      headers: { ...headers, 'X-Natamux-Task': id, 'X-Natamux-Terminal': '2' },
      data: { hook_event_name: name },
    })
  await hook('UserPromptSubmit')
  await expect.poll(async () => (await lane()).state, { timeout: 10_000 }).toBe('running')
  await hook('Stop')
  await expect.poll(async () => (await lane()).state, { timeout: 10_000 }).toBe('needs-input')

  const waiting = page.getByRole('button', { name: 'Shell 2, Needs input', exact: true })
  await expect(waiting).toBeVisible({ timeout: 15_000 })
  await expect(waiting.locator('[data-state="needs-input"]')).toBeVisible()
  await expect(waiting.locator('[data-tip="State not known yet"]')).toHaveCount(0)
  // The hooks of Shell 2 leave the task alone.
  const task = (await (await request.get(`/api/tasks/${id}`, { headers })).json()).task
  expect(task.state).toBe('review')
})

// C1 and C8 at phone width: nothing scrolls sideways.
test('fits a phone without sideways scroll', async ({ page, request }) => {
  const title = `Sidebar phone ${'long title '.repeat(8)}${Date.now()}`
  await heldTask(request, title)
  await page.setViewportSize({ width: 390, height: 844 })
  await page.reload()

  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  )
  expect(overflow).toBeLessThanOrEqual(0)
})
