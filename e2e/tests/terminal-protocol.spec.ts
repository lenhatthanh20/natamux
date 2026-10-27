import { expect, type Page, test } from '@playwright/test'
import { headers, openTask, signIn, type TaskEvent, taskOf } from './helpers'
import {
  controlTerminalSockets,
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
  shellPid,
  watchReconnecting,
} from './terminal-helpers'

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

async function openTerminal(page: Page, title: string, text: string): Promise<void> {
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Terminal 1'))).toContainText(text)
}

// AC 9, C10
test('shows no history scrolling by while a snapshot is replayed', async ({ page, request }) => {
  test.setTimeout(120_000)
  const task = await makeShellTask(request, `Terminal e2e snapshot ${Date.now()}`)
  made.push(task.id)
  await signIn(page)
  await openTerminal(page, task.title, '$')
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  const marker = await fillHistory(page, shell, { lines: 10_000, sgr: false, id: 7 })

  // A fresh page attaches again and gets a snapshot of the full history.
  await installScreenSampler(page, { label: 'Terminal 1', marker })
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
  const shell = pane(page, 'Terminal 1')
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

// The numbers of the TICK lines on the screen, in order.
function ticksOn(text: string): number[] {
  return [...text.matchAll(/TICK-(\d+)/g)].map((match) => Number(match[1]))
}

// AC 27, C29: a socket closed by the network comes back by itself, to the same shell, with only
// the output it missed.
test('reconnects a lost socket and resumes the same shell in place', async ({ page, request }) => {
  test.setTimeout(120_000)
  const task = await makeShellTask(request, `Terminal e2e reconnect ${Date.now()}`)
  made.push(task.id)
  const control = await controlTerminalSockets(page)
  await signIn(page)
  await openTerminal(page, task.title, '$')
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  const pid = await shellPid(page, shell, 'PIDA')
  await runIn(page, shell, 'for i in $(seq 1 600); do echo TICK-$i; sleep 0.1; done')
  await expect(rows(shell)).toContainText('TICK-5')
  const reconnecting = await watchReconnecting(page)

  const lost = control.last()
  const dropped = Date.now()
  await lost.close()
  await expect.poll(() => control.sockets.length, { timeout: 10_000 }).toBeGreaterThan(1)
  const back = control.last()
  await expect
    .poll(() => back.got.find((message) => message.type === 'replayed'), { timeout: 10_000 })
    .toBeDefined()

  expect(await reconnecting()).toBe(true)
  // The same shell, resumed from the ring: no snapshot, so no reset.
  expect(back.got.slice(0, 2).map((message) => message.type)).toEqual(['hello', 'resume'])
  expect(back.got[0]?.shellId).toBe(lost.got[0]?.shellId)
  expect(back.got.map((message) => message.type)).not.toContain('snapshot')
  const replayedAt = back.got.find((message) => message.type === 'replayed')?.at ?? 0
  expect(replayedAt - dropped).toBeLessThan(2_000)
  await expect(shell.getByText('Reconnecting.')).toHaveCount(0)

  // The ticks printed while the socket was closed are on the screen, with no gap.
  const seen = ticksOn((await rows(shell).textContent()) ?? '').at(-1) ?? 0
  await expect(rows(shell)).toContainText(`TICK-${seen + 5}`)
  const ticks = ticksOn((await rows(shell).textContent()) ?? '')
  expect(ticks.length).toBeGreaterThan(5)
  expect(ticks).toEqual(ticks.map((_, index) => (ticks[0] ?? 0) + index))

  await shell.getByRole('region', { name: 'Terminal screen' }).click()
  await page.keyboard.press('Control+C')
  await expect(rows(shell)).toContainText(PROMPT.trim())
  expect(await shellPid(page, shell, 'PIDB')).toBe(pid)
})

// AC 30, C32
test('shows "The terminal ended." after exit, and opens no new socket', async ({
  page,
  request,
}) => {
  test.setTimeout(90_000)
  const task = await makeShellTask(request, `Terminal e2e exit ${Date.now()}`)
  made.push(task.id)
  const control = await controlTerminalSockets(page)
  await signIn(page)
  await openTerminal(page, task.title, '$')
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  const count = control.sockets.length

  await runIn(page, shell, 'exit')
  await expect(shell.getByText('The terminal ended.')).toBeVisible()
  await expect(shell.getByRole('button', { name: 'Open a new terminal' })).toBeVisible()
  // Longer than the first retry delays would be.
  await page.waitForTimeout(3_000)
  expect(control.sockets.length).toBe(count)
  expect(control.last().got.at(-1)?.type).toBe('exit')
})

// AC 12, the part with a resume (C14): replies to queries printed while the socket was closed are
// never sent, and a real Shift+F3 typed during the resume is, once.
test('drops replies to queries replayed by a resume, and keeps a real Shift+F3', async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const task = await makeShellTask(request, `Terminal e2e resume queries ${Date.now()}`)
  made.push(task.id)
  const control = await controlTerminalSockets(page)
  await signIn(page)
  await openTerminal(page, task.title, '$')
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  // A job prints a DA1 and a CPR query in 5 s, while the socket is closed. Raw mode with no echo:
  // cat -v shows each byte it gets, so the screen shows what the page sent.
  await runIn(
    page,
    shell,
    "(sleep 5; printf '\\033[c\\033[6n') & stty raw -echo; echo READY-$((1+1)); exec cat -v",
  )
  await expect(rows(shell)).toContainText('READY-2')

  control.block(true)
  await control.last().close()
  const hold = control.holdNextReplayed()
  // Past the job's printf.
  await page.waitForTimeout(6_500)
  control.block(false)
  await expect.poll(hold.isHeld, { timeout: 30_000 }).toBe(true)
  const back = control.last()
  expect(back.got.map((message) => message.type).slice(0, 2)).toEqual(['hello', 'resume'])

  await shell.locator('.xterm-helper-textarea').focus()
  await page.keyboard.press('Shift+F3')
  // Nothing is sent while the catch-up runs: no reply, and the key waits.
  expect(back.sent.filter((frame) => frame.type === 'input')).toEqual([])

  hold.release()
  await expect(rows(shell)).toContainText('^[[1;2R')
  const text = (await rows(shell).textContent()) ?? ''
  // No DA reply (^[[?…c) and no CPR with the real cursor position.
  expect(text).not.toContain('^[[?')
  expect(text.match(/\^\[\[\d+;\d+R/g)).toEqual(['^[[1;2R'])
  const typed = back.sent
    .filter((frame) => frame.type === 'input')
    .map((frame) => frame.data)
    .join('')
  expect(typed).toBe('\x1b[1;2R')
})

// T-0202: an agent that ended its turn, in a terminal nobody looks at, pauses after the idle
// minutes (30 s here, see compose.e2e.yaml). Opening the terminal starts it on its session again.
test('pauses an idle agent nobody looks at, and starts it again when shown', async ({
  page,
  request,
}) => {
  test.setTimeout(150_000)
  const repos = await (await request.get('/api/repos', { headers })).json()
  // [no-done]: the fake agent ends its turn without `natamux done`, so the stage waits for it.
  const title = `[no-done] Idle pause ${Date.now()}`
  const created = await request.post('/api/tasks', {
    headers,
    data: { title, repo: repos.repos[0].id },
  })
  const { task } = await created.json()
  made.push(task.id)
  await expect
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
    .toMatch(/^(ready|review)$/)
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: { source: 'human', type: 'state.overridden', payload: { to: 'ready', stage: 'spec' } },
  })
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })

  const pausedEvent = async () => {
    const body = await (await request.get(`/api/tasks/${task.id}/events`, { headers })).json()
    const events: TaskEvent[] = body.events.map((entry: { event: TaskEvent }) => entry.event)
    return events.find((event) => event.type === 'agent.paused')?.payload ?? null
  }
  await expect
    .poll(pausedEvent, { timeout: 100_000, intervals: [2_000] })
    .toMatchObject({ terminal: '1', idleMinutes: 30, session: expect.stringMatching(/^fake-/) })

  await signIn(page)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shown = page.getByRole('region', { name: 'Terminal screen' }).first()
  await expect(shown.locator('.xterm-rows')).toContainText('(resumed session fake-', {
    timeout: 30_000,
  })
})
