import { expect, type Page, test } from '@playwright/test'
import { openTask, signIn } from './helpers'
import {
  fillHistory,
  holdReplayed,
  installScreenSampler,
  makeShellTask,
  PROMPT,
  pane,
  preparePrompt,
  removeTasks,
  rows,
  runIn,
  screenFrames,
} from './terminal-helpers'

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

async function openTerminal(page: Page, title: string, text: string): Promise<void> {
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Shell 1'))).toContainText(text)
}

// AC 9, C10
test('shows no history scrolling by while a snapshot is replayed', async ({ page, request }) => {
  test.setTimeout(120_000)
  const task = await makeShellTask(request, `Terminal e2e snapshot ${Date.now()}`)
  made.push(task.id)
  await signIn(page)
  await openTerminal(page, task.title, '$')
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)
  const marker = await fillHistory(page, shell, { lines: 10_000, sgr: false, id: 7 })

  // A fresh page attaches again and gets a snapshot of the full history.
  await installScreenSampler(page, { label: 'Shell 1', marker })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
  await openTerminal(page, task.title, marker)
  await expect(page.getByRole('status', { name: 'Loading the terminal' })).toHaveCount(0)
  await expect(rows(shell)).toContainText(PROMPT.trim())
  // The last frame is drawn after the skeleton is gone.
  await expect
    .poll(async () => (await screenFrames(page)).at(-1)?.visible, { timeout: 5_000 })
    .toBe(true)

  const frames = await screenFrames(page)
  // Rows that were drawn with text always had the final screen, never a part of the history.
  const drawn = frames.filter((frame) => frame.visible && frame.hasText)
  expect(drawn.length).toBeGreaterThan(0)
  expect(drawn.filter((frame) => !frame.hasMarker)).toEqual([])
  // The catch-up was seen: hidden rows with the skeleton.
  expect(frames.some((frame) => !frame.visible && frame.skeleton)).toBe(true)
})

// AC 12, the part with real keys (C14): the reply of a real Shift+F3 looks like a query reply,
// and is still sent, once, after the catch-up and before later keys.
test('sends keys typed while a snapshot is held once, after it, in order', async ({
  page,
  request,
}) => {
  test.setTimeout(90_000)
  const task = await makeShellTask(request, `Terminal e2e held keys ${Date.now()}`)
  made.push(task.id)
  await signIn(page)
  await openTerminal(page, task.title, '$')
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)
  // Raw mode with no echo: cat -v shows each byte it gets, so the screen shows what arrived.
  await runIn(page, shell, 'stty raw -echo; echo READY-$((1+1)); exec cat -v')
  await expect(rows(shell)).toContainText('READY-2')

  const hold = await holdReplayed(page)
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await hold.held

  await shell.locator('.xterm-helper-textarea').focus()
  await page.keyboard.press('Shift+F3')
  await page.keyboard.type('xy')
  // Nothing is sent while the catch-up runs.
  expect(hold.clientFrames.filter((frame) => frame.type === 'input')).toEqual([])

  hold.release()
  await expect(rows(shell)).toContainText('^[[1;2Rxy')
  const text = (await rows(shell).textContent()) ?? ''
  expect(text.split('^[[1;2R').length - 1).toBe(1)
  const typed = hold.clientFrames
    .filter((frame) => frame.type === 'input')
    .map((frame) => frame.data)
    .join('')
  expect(typed).toBe('\x1b[1;2Rxy')
})
