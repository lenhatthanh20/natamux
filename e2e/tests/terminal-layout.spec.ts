import { type APIRequestContext, expect, type Page, test } from '@playwright/test'
import { headers, openTask, signIn } from './helpers'
import { makeShellTask, pane, removeTasks, rows, runIn } from './terminal-helpers'

// T-0089. A pane that changes size (New shell, Close shell, a banner of a stage change) hides its
// screen until the shell drew for the new size, then fades it in. No frame shows the screen at a
// size between the old and the final one.

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

// Size changes the dashboard sends, per shell, after the size of its connect.
function watchSizes(page: Page): string[] {
  const seen: string[] = []
  page.on('websocket', (socket) => {
    const url = new URL(socket.url())
    if (!url.pathname.endsWith('/terminal')) {
      return
    }
    const name = url.searchParams.get('terminal')
    let size = `${url.searchParams.get('cols')}x${url.searchParams.get('rows')}`
    socket.on('framesent', ({ payload }) => {
      if (typeof payload !== 'string') {
        return
      }
      const message = JSON.parse(payload) as { type: string; cols?: number; rows?: number }
      if ((message.type !== 'resize' && message.type !== 'visible') || message.cols === undefined) {
        return
      }
      const next = `${message.cols}x${message.rows}`
      if (next !== size) {
        seen.push(`${name}: ${size} -> ${next}`)
        size = next
      }
    })
  })
  return seen
}

type Frame = { at: number; visible: boolean; size: string; fade: boolean }

// Notes Shell 1's screen on every animation frame for a while: drawn or not, its rows and width.
async function sampleShell1(page: Page): Promise<void> {
  await page.evaluate(() => {
    const frames: Frame[] = []
    ;(window as unknown as { __e2eLayout: Frame[] }).__e2eLayout = frames
    const start = performance.now()
    const tick = () => {
      const screen = document.querySelector(
        'section[aria-label="Shell 1"] section[aria-label="Terminal screen"]',
      )
      const rowsElement = screen?.querySelector('.xterm-rows')
      const drawn = screen?.querySelector('.xterm-screen')
      const host = screen?.querySelector('.xterm')?.parentElement
      if (rowsElement && drawn instanceof HTMLElement && host) {
        frames.push({
          at: Math.round(performance.now() - start),
          visible: getComputedStyle(rowsElement).visibility === 'visible',
          size: `${rowsElement.children.length}x${drawn.offsetWidth}`,
          fade: host.classList.contains('hm-fade-in'),
        })
      }
      if (performance.now() - start < 1500) {
        requestAnimationFrame(tick)
      }
    }
    requestAnimationFrame(tick)
  })
}

async function shell1Frames(page: Page): Promise<Frame[]> {
  await page.waitForTimeout(1600)
  return page.evaluate(() => (window as unknown as { __e2eLayout: Frame[] }).__e2eLayout)
}

// Up to the first hidden frame the screen keeps its old size; after it, every drawn frame has the
// final size. The screen fades in, and is hidden a short time only.
function expectCalm(frames: Frame[]): void {
  const final = frames.at(-1)
  expect(final?.visible).toBe(true)
  const start = frames.findIndex((frame) => !frame.visible)
  expect(start).toBeGreaterThan(0)
  const old = frames[0]?.size
  expect(frames.slice(0, start).filter((frame) => frame.size !== old)).toEqual([])
  expect(
    frames.slice(start).filter((frame) => frame.visible && frame.size !== final?.size),
  ).toEqual([])
  const hidden = frames.filter((frame) => !frame.visible)
  // FIT_DELAY_MS + REDRAW_MAX_MS is 500 ms; the rest is room for a slow CI machine.
  expect((hidden.at(-1)?.at ?? 0) - (hidden[0]?.at ?? 0)).toBeLessThan(800)
  expect(frames.some((frame) => frame.visible && frame.fade)).toBe(true)
}

async function setState(request: APIRequestContext, id: string, to: string): Promise<void> {
  await request.post(`/api/tasks/${id}/events`, {
    headers,
    data: { source: 'human', type: 'state.overridden', payload: { to, stage: 'context' } },
  })
}

async function openShells(page: Page, request: APIRequestContext, purpose: string) {
  const task = await makeShellTask(request, `Terminal e2e layout ${purpose} ${Date.now()}`)
  made.push(task.id)
  await page.setViewportSize({ width: 1440, height: 900 })
  await signIn(page)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Shell 1')
  await expect(rows(shell)).toContainText('$')
  await runIn(page, shell, 'seq 1 100')
  await expect(rows(shell)).toContainText('100')
  return { task, shell }
}

test('shows Shell 1 at its final size only, after New shell and Close shell', async ({
  page,
  request,
}) => {
  const sizes = watchSizes(page)
  await openShells(page, request, 'shells')

  let before = sizes.length
  await sampleShell1(page)
  await page.getByRole('button', { name: 'New shell' }).click()
  await expect(rows(pane(page, 'Shell 2'))).toContainText('$', { timeout: 30_000 })
  expectCalm(await shell1Frames(page))
  // One new size for Shell 1. Shell 2 starts at its final size, also after "Connecting." goes.
  expect(sizes.slice(before)).toEqual([expect.stringMatching(/^1: /)])

  before = sizes.length
  await sampleShell1(page)
  await pane(page, 'Shell 2').getByRole('button', { name: 'Close Shell 2' }).click()
  expectCalm(await shell1Frames(page))
  expect(sizes.slice(before)).toEqual([expect.stringMatching(/^1: /)])
})

test('shows Shell 1 at its final size only, when a stage banner comes and goes', async ({
  page,
  request,
}) => {
  const sizes = watchSizes(page)
  const { task } = await openShells(page, request, 'banner')
  const banner = page.getByRole('main').getByText(/^Ready for review/)
  await expect(banner).toBeVisible()

  let before = sizes.length
  await sampleShell1(page)
  await setState(request, task.id, 'running')
  await expect(banner).toBeHidden()
  expectCalm(await shell1Frames(page))
  expect(sizes.slice(before)).toEqual([expect.stringMatching(/^1: /)])

  before = sizes.length
  await sampleShell1(page)
  await setState(request, task.id, 'review')
  await expect(banner).toBeVisible()
  expectCalm(await shell1Frames(page))
  expect(sizes.slice(before)).toEqual([expect.stringMatching(/^1: /)])
})
