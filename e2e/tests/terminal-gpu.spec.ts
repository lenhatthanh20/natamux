import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test'
import { openTask, signIn, taskNamed } from './helpers'
import { goToAllTasks, makeShellTask, pane, removeTasks, runIn } from './terminal-helpers'

// The other specs run without WebGL, so their text is in the page. This one draws with WebGL
// (SwiftShader, a GPU in software) to check the GPU budget: a terminal hidden in the holder gives
// its renderer back (to a new terminal, or after the give-back delay), the renderer changes while
// the terminal has no size, and lost contexts turn WebGL off. The screen is then a canvas, so the test counts drawn pixels instead of reading text.

// GPU_CONTEXTS of packages/web/src/features/terminals/gpu-renderer.ts and MAX_PANES of
// lib/panes.ts. e2e cannot import web code.
const GPU_CONTEXTS = 8
const MAX_PANES = 6
// GIVE_BACK_DELAY_MS of the same file (10 s), and a little more.
const GIVE_BACK_WAIT_MS = 10_500
// A pane with only a cursor has about 150 drawn pixels; a prompt and lines of text have thousands.
const DRAWN_PIXELS = 1500
const PROMPT_PIXELS = 300
// xterm waits 3 s for a lost context to come back before it reports the loss.
const CONTEXT_LOSS_WAIT_MS = 4_500

test.use({ launchOptions: { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] } })

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

async function newTask(request: APIRequestContext, purpose: string) {
  const task = await makeShellTask(request, `Terminal e2e gpu ${purpose} ${Date.now()}`)
  made.push(task.id)
  return task
}

function screenOf(target: Locator): Locator {
  return target.getByRole('region', { name: 'Terminal screen' })
}

// Pixels of the pane's screen that differ from its background (the top left corner, which is
// padding). The screenshot is decoded in the page, as Node has no image decoder.
async function drawnPixels(page: Page, target: Locator): Promise<number> {
  const png = await screenOf(target).screenshot()
  return page.evaluate(async (data) => {
    const image = new Image()
    image.src = `data:image/png;base64,${data}`
    await image.decode()
    const canvas = document.createElement('canvas')
    canvas.width = image.width
    canvas.height = image.height
    const context = canvas.getContext('2d')
    if (context === null) {
      return 0
    }
    context.drawImage(image, 0, 0)
    const pixels = context.getImageData(0, 0, image.width, image.height).data
    const [r, g, b] = [pixels[0] ?? 0, pixels[1] ?? 0, pixels[2] ?? 0]
    let drawn = 0
    for (let index = 0; index < pixels.length; index += 4) {
      const distance =
        Math.abs((pixels[index] ?? 0) - r) +
        Math.abs((pixels[index + 1] ?? 0) - g) +
        Math.abs((pixels[index + 2] ?? 0) - b)
      if (distance > 60) {
        drawn += 1
      }
    }
    return drawn
  }, png.toString('base64'))
}

async function expectDrawn(page: Page, target: Locator, least = DRAWN_PIXELS): Promise<void> {
  await expect.poll(() => drawnPixels(page, target), { timeout: 30_000 }).toBeGreaterThan(least)
}

// "webgl" while the pane draws with a WebGL canvas, "dom" when xterm draws rows of text.
async function rendererOf(target: Locator): Promise<'webgl' | 'dom'> {
  return screenOf(target).evaluate((element) => {
    for (const canvas of element.querySelectorAll('canvas')) {
      if (canvas.getContext('webgl2') !== null) {
        return 'webgl'
      }
    }
    return 'dom'
  })
}

// WebGL canvases of the hidden terminals. The holder is aria-hidden, so it is read directly.
async function heldWebglCanvases(page: Page): Promise<number> {
  return page.locator('#terminal-holder').evaluate((holder) => {
    let count = 0
    for (const canvas of holder.querySelectorAll('canvas')) {
      if (canvas.getContext('webgl2') !== null) {
        count += 1
      }
    }
    return count
  })
}

function shellPanes(page: Page): Locator {
  return page.getByRole('region', { name: /^Shell \d+$/ })
}

// Opens the task's Terminal tab with `count` shells, each with lines of text from its own name.
async function openShells(page: Page, title: string, count: number): Promise<void> {
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  for (let index = 2; index <= count; index++) {
    await page.getByRole('button', { name: 'New shell' }).click()
    await expect(shellPanes(page)).toHaveCount(index)
  }
  for (let index = 1; index <= count; index++) {
    const shell = pane(page, `Shell ${index}`)
    // Keys typed before the prompt would be lost.
    await expectDrawn(page, shell, PROMPT_PIXELS)
    await runIn(page, shell, `clear; seq -f 'gpu shell ${index} line %g' 1 20`)
    await expectDrawn(page, shell)
  }
}

async function expectAllDrawn(page: Page): Promise<void> {
  const count = await shellPanes(page).count()
  for (let index = 1; index <= count; index++) {
    await expectDrawn(page, pane(page, `Shell ${index}`))
  }
}

async function loseContext(target: Locator): Promise<void> {
  await screenOf(target).evaluate((element) => {
    for (const canvas of element.querySelectorAll('canvas')) {
      canvas.getContext('webgl2')?.getExtension('WEBGL_lose_context')?.loseContext()
    }
  })
}

test('gives GPU renderers back and changes renderers while hidden, with no blank pane', async ({
  page,
  request,
}) => {
  test.setTimeout(150_000)
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  const taskA = await newTask(request, 'A')
  const taskB = await newTask(request, 'B')
  await signIn(page)
  // Console errors count from here: the first load logs a CSP note that has nothing to do with
  // terminals.
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text())
    }
  })
  const hasWebgl = await page.evaluate(
    () => document.createElement('canvas').getContext('webgl2') !== null,
  )
  expect(hasWebgl, 'the browser must draw with WebGL for this test').toBe(true)

  // A full grid of shells.
  await openShells(page, taskA.title, MAX_PANES)
  for (let index = 1; index <= MAX_PANES; index++) {
    expect(await rendererOf(pane(page, `Shell ${index}`))).toBe('webgl')
  }

  // The rest of the budget, and one more terminal, which takes the renderer of a hidden one in
  // the display: none holder.
  const shellsOfB = GPU_CONTEXTS - MAX_PANES + 1
  await goToAllTasks(page)
  await openShells(page, taskB.title, shellsOfB)
  for (let index = 1; index <= shellsOfB; index++) {
    expect(await rendererOf(pane(page, `Shell ${index}`))).toBe('webgl')
  }

  // Back and forth: each switch makes a hidden terminal give its renderer back.
  for (let round = 0; round < 3; round++) {
    await goToAllTasks(page)
    await openTask(page, taskA.title)
    await expect(shellPanes(page)).toHaveCount(MAX_PANES)
    await expectAllDrawn(page)
    await goToAllTasks(page)
    await openTask(page, taskB.title)
    await expectAllDrawn(page)
  }

  // Another tab and back.
  await goToAllTasks(page)
  const tabs = await openTask(page, taskA.title)
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expectAllDrawn(page)

  // Hidden longer than the give-back delay, the terminals free their GPU renderers, and take
  // them again when shown.
  await goToAllTasks(page)
  expect(await heldWebglCanvases(page)).toBeGreaterThan(0)
  await page.waitForTimeout(GIVE_BACK_WAIT_MS)
  expect(await heldWebglCanvases(page)).toBe(0)
  await openTask(page, taskA.title)
  await expect(shellPanes(page)).toHaveCount(MAX_PANES)
  for (let index = 1; index <= MAX_PANES; index++) {
    expect(await rendererOf(pane(page, `Shell ${index}`))).toBe('webgl')
  }
  await expectAllDrawn(page)

  // Two lost contexts turn WebGL off for the page; those panes go on with the DOM renderer.
  for (const name of ['Shell 1', 'Shell 2']) {
    const shell = pane(page, name)
    expect(await rendererOf(shell)).toBe('webgl')
    await loseContext(shell)
    await page.waitForTimeout(CONTEXT_LOSS_WAIT_MS)
    await expect.poll(() => rendererOf(shell)).toBe('dom')
    await expectDrawn(page, shell)
  }
  await goToAllTasks(page)
  await openTask(page, taskB.title)
  await expectAllDrawn(page)
  await goToAllTasks(page)
  await openTask(page, taskA.title)
  await expectAllDrawn(page)
  expect(await rendererOf(pane(page, 'Shell 1'))).toBe('dom')

  expect(errors).toEqual([])
})

// Keeps every WebGL context the page makes, so a test can count new ones and see that a context
// given back is lost. Call before signIn.
async function watchWebglContexts(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const made: WebGL2RenderingContext[] = []
    ;(window as unknown as { __e2eWebgl: WebGL2RenderingContext[] }).__e2eWebgl = made
    const original = HTMLCanvasElement.prototype.getContext as (...args: unknown[]) => unknown
    const patched = function (this: HTMLCanvasElement, ...args: unknown[]) {
      const context = original.apply(this, args)
      if (context instanceof WebGL2RenderingContext && !made.includes(context)) {
        made.push(context)
      }
      return context
    }
    HTMLCanvasElement.prototype.getContext = patched as HTMLCanvasElement['getContext']
  })
}

async function webglMade(page: Page): Promise<number> {
  return page.evaluate(() => (window as unknown as { __e2eWebgl: unknown[] }).__e2eWebgl.length)
}

// isContextLost() of each context whose canvas left the page: a renderer given back or disposed.
async function droppedContextsLost(page: Page): Promise<boolean[]> {
  return page.evaluate(() => {
    const made = (window as unknown as { __e2eWebgl: WebGL2RenderingContext[] }).__e2eWebgl
    const dropped = made.filter((context) => !(context.canvas as HTMLCanvasElement).isConnected)
    return dropped.map((context) => context.isContextLost())
  })
}

// T-0066 acceptance 4 and 5. Two tasks of 4 shells fit the budget, so switching between them
// makes no new context; a renderer given back loses its context at once, not at garbage
// collection. The addon's link layer is a 2D canvas, so it never shows up here.
test('keeps the renderers of two tasks of 4 shells, and frees a context given back', async ({
  page,
  request,
}) => {
  test.setTimeout(150_000)
  const taskA = await newTask(request, 'four A')
  const taskB = await newTask(request, 'four B')
  await watchWebglContexts(page)
  await signIn(page)
  await openShells(page, taskA.title, 4)
  await goToAllTasks(page)
  await openShells(page, taskB.title, 4)

  const sidebar = page.getByRole('navigation', { name: 'Workspaces' })
  const show = async (title: string) => {
    await sidebar.getByRole('button', { name: taskNamed(title) }).click()
    await expect(shellPanes(page)).toHaveCount(4)
    for (let index = 1; index <= 4; index++) {
      expect(await rendererOf(pane(page, `Shell ${index}`))).toBe('webgl')
    }
  }
  // The first visit of each: task A may have been hidden past the give-back delay meanwhile.
  await show(taskA.title)
  await show(taskB.title)
  const made = await webglMade(page)
  const start = Date.now()
  for (const task of [taskA, taskB, taskA, taskB]) {
    await show(task.title)
  }
  // Within the give-back delay, so no hidden renderer was given back for its age.
  const took = Date.now() - start
  expect(took).toBeLessThan(GIVE_BACK_WAIT_MS - 1000)
  expect(await webglMade(page)).toBe(made)

  await goToAllTasks(page)
  await page.waitForTimeout(GIVE_BACK_WAIT_MS)
  const lost = await droppedContextsLost(page)
  expect(lost.length).toBeGreaterThanOrEqual(8)
  expect(lost.every(Boolean)).toBe(true)
})
