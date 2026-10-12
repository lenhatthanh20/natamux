import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test'
import { openTask, signIn } from './helpers'
import { goToAllTasks, makeShellTask, pane, removeTasks, runIn } from './terminal-helpers'

// The other specs run without WebGL, so their text is in the page. This one draws with WebGL
// (SwiftShader, a GPU in software) to check the GPU budget: a terminal hidden in the holder gives
// its renderer back, the renderer changes while the terminal has no size, and lost contexts turn
// WebGL off. The screen is then a canvas, so the test counts drawn pixels instead of reading text.

// GPU_CONTEXTS of packages/web/src/lib/gpu-renderer.ts. e2e cannot import web code.
const GPU_CONTEXTS = 6
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
  test.setTimeout(300_000)
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text())
    }
  })
  const taskA = await newTask(request, 'A')
  const taskB = await newTask(request, 'B')
  await signIn(page)
  const hasWebgl = await page.evaluate(
    () => document.createElement('canvas').getContext('webgl2') !== null,
  )
  expect(hasWebgl, 'the browser must draw with WebGL for this test').toBe(true)

  // Six shells take the whole budget.
  await openShells(page, taskA.title, GPU_CONTEXTS)
  for (let index = 1; index <= GPU_CONTEXTS; index++) {
    expect(await rendererOf(pane(page, `Shell ${index}`))).toBe('webgl')
  }

  // A seventh terminal takes the renderer of a hidden one, which is in the display: none holder.
  await goToAllTasks(page)
  await openShells(page, taskB.title, 1)
  expect(await rendererOf(pane(page, 'Shell 1'))).toBe('webgl')

  // Back and forth: each switch makes a hidden terminal give its renderer back.
  for (let round = 0; round < 3; round++) {
    await goToAllTasks(page)
    await openTask(page, taskA.title)
    await expect(shellPanes(page)).toHaveCount(GPU_CONTEXTS)
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

  expect(errors.filter((text) => !text.includes('WebGL context was lost'))).toEqual([])
})
