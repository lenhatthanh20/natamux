import { expect, type Locator, type Page, test } from '@playwright/test'
import { DEMO_TITLE, openTask, signIn } from './helpers'
import { addShells, pane, rows, runIn } from './terminal-helpers'

// The look of the terminal: docs/design-terminal-look.md, cases in the T-0005 test spec.

const ICON_FONT = 'Symbols Nerd Font Mono'
const MAIN_FONT = 'JetBrains Mono Variable'
const BACKGROUND = { Dark: 'rgb(15, 18, 25)', Light: 'rgb(247, 249, 253)' }

async function openDemoTerminal(page: Page): Promise<void> {
  const tabs = await openTask(page, DEMO_TITLE)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Shell 1'))).toContainText('$', { timeout: 30_000 })
}

test.beforeEach(async ({ page }) => {
  await signIn(page)
  await openDemoTerminal(page)
})

function screenOf(shell: Locator): Locator {
  return shell.getByRole('region', { name: 'Terminal screen' })
}

// The dim layer of a pane's screen: its opacity, or 'none' when the pane is not dimmed.
async function dimOf(shell: Locator): Promise<string> {
  return shell.locator('.hm-terminal').evaluate((element) => {
    const after = getComputedStyle(element, '::after')
    return after.content === 'none' ? 'none' : after.opacity
  })
}

// The load status of a font face, or 'missing' when the page has none.
async function faceStatus(page: Page, family: string, style = 'normal'): Promise<string> {
  return page.evaluate(
    ([name, wanted]) => {
      let status = 'missing'
      document.fonts.forEach((face) => {
        if (face.family.includes(name) && face.style === wanted && status !== 'loaded') {
          status = face.status
        }
      })
      return status
    },
    [family, style] as const,
  )
}

async function rowsFont(shell: Locator): Promise<string> {
  return rows(shell).evaluate((element) => getComputedStyle(element).fontFamily)
}

async function setTheme(page: Page, theme: 'Dark' | 'Light'): Promise<void> {
  await page.getByRole('button', { name: 'Account' }).click()
  await page.getByRole('menuitemcheckbox', { name: theme }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase())
}

test('draws in JetBrains Mono at 13px, with the icon font next (C1, C5)', async ({ page }) => {
  const shell = pane(page, 'Shell 1')
  const font = await rowsFont(shell)
  expect(font.startsWith(`"${MAIN_FONT}", "${ICON_FONT}", "Geist Mono Variable"`)).toBe(true)
  expect(await rows(shell).evaluate((element) => getComputedStyle(element).fontSize)).toBe('13px')
  await expect
    .poll(() => page.evaluate((name) => document.fonts.check(`13px "${name}"`), MAIN_FONT))
    .toBe(true)

  // The rest of the UI keeps Geist Mono.
  const mono = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim(),
  )
  expect(mono.startsWith('"Geist Mono Variable"')).toBe(true)
})

test('draws Vietnamese and italic text with JetBrains Mono faces (C6, C7)', async ({ page }) => {
  const shell = pane(page, 'Shell 1')
  // Printed from UTF-8 bytes: typed into the shell of the image, these letters get lost.
  const vietnamese = 'Tiếng Việt: ắ ằ ẳ ẵ ặ ơ ư đ ẮẰẲẴẶ ƠƯĐ'
  const bytes = Array.from(new TextEncoder().encode(vietnamese))
    .map((byte) => (byte < 128 ? String.fromCharCode(byte) : `\\x${byte.toString(16)}`))
    .join('')
  await runIn(page, shell, `printf '${bytes}\\n'`)
  await expect(rows(shell)).toContainText(vietnamese)
  await expect
    .poll(() => page.evaluate((name) => document.fonts.check(`13px "${name}"`, 'ắƠđ'), MAIN_FONT))
    .toBe(true)

  await runIn(page, shell, "printf '\\033[3mitalic\\033[0m done\\n'")
  await expect(rows(shell)).toContainText('italic done')
  await expect.poll(() => faceStatus(page, MAIN_FONT, 'italic')).toBe('loaded')
})

test('puts a picked font first, then goes back to the built-in one (C3, C4)', async ({ page }) => {
  await page.evaluate(() =>
    localStorage.setItem('natamux.terminal-font', JSON.stringify({ family: 'DejaVu Sans Mono' })),
  )
  // The reload opens the same task again.
  await page.reload()
  await page
    .getByRole('tablist', { name: 'View', exact: true })
    .getByRole('tab', { name: 'Terminal' })
    .click()
  const shell = pane(page, 'Shell 1')
  await expect(rows(shell)).toContainText('$', { timeout: 30_000 })
  expect(await rowsFont(shell)).toMatch(/^"DejaVu Sans Mono", "JetBrains Mono Variable"/)

  // No reload: the terminal takes the built-in font at once.
  await page.getByRole('button', { name: 'Account' }).click()
  await page.getByRole('menuitem', { name: 'Terminal font…' }).click()
  await page.getByRole('button', { name: 'Use built-in font' }).click()
  await expect.poll(() => rowsFont(shell)).toMatch(/^"JetBrains Mono Variable"/)

  await page.getByRole('button', { name: 'Account' }).click()
  await page.getByRole('menuitem', { name: 'Terminal font…' }).click()
  await expect(page.getByText(/built-in font, JetBrains Mono, 13px/)).toBeVisible()
  await page.getByRole('button', { name: 'Cancel' }).click()
})

test('shows a block cursor when focused and an outline one when not (C11)', async ({ page }) => {
  await addShells(page, 1)
  const first = pane(page, 'Shell 1')
  await screenOf(first).click()
  const cursor = rows(first).locator('.xterm-cursor')
  await expect(cursor).toHaveClass(/xterm-cursor-block/)
  await expect(cursor).toHaveClass(/xterm-cursor-blink/)

  await screenOf(pane(page, 'Shell 2')).click()
  await expect(cursor).toHaveClass(/xterm-cursor-outline/)
  await expect(cursor).not.toHaveClass(/xterm-cursor-blink/)
})

test('dims the selection of a terminal without the focus (C15)', async ({ page }) => {
  await addShells(page, 1)
  const first = pane(page, 'Shell 1')
  await runIn(page, first, 'clear; echo selectme')
  await expect(rows(first)).toContainText('selectme')
  // Playwright never finds xterm's text element ready for a click, so the mouse goes to its place.
  const word = await rows(first).getByText('selectme', { exact: true }).boundingBox()
  await page.mouse.dblclick((word?.x ?? 0) + 10, (word?.y ?? 0) + (word?.height ?? 0) / 2)
  const selection = first.locator('.xterm-selection div').first()
  await expect(selection).toBeVisible()
  const focused = await selection.evaluate((element) => getComputedStyle(element).backgroundColor)

  await screenOf(pane(page, 'Shell 2')).click()
  await expect
    .poll(() => selection.evaluate((element) => getComputedStyle(element).backgroundColor))
    .not.toBe(focused)
})

test('keeps even padding, with the scrollbar in the right one (C16)', async ({ page }) => {
  const shell = pane(page, 'Shell 1')
  await runIn(page, shell, 'seq 1 300')
  await expect(rows(shell)).toContainText('300')
  const { section, screen, lastRow, bar } = await screenOf(shell).evaluate((element) => {
    const box = (target: Element | null | undefined) => {
      const rect = target?.getBoundingClientRect()
      return {
        left: rect?.left ?? 0,
        right: rect?.right ?? 0,
        top: rect?.top ?? 0,
        bottom: rect?.bottom ?? 0,
        width: rect?.width ?? 0,
      }
    }
    const rowList = element.querySelectorAll('.xterm-rows > div')
    return {
      section: box(element),
      screen: box(element.querySelector('.xterm-screen')),
      lastRow: box(rowList[rowList.length - 1]),
      bar: box(element.querySelector('.xterm-scrollable-element > .scrollbar.vertical')),
    }
  })
  expect(Math.round(screen.left - section.left)).toBe(10)
  expect(Math.round(screen.top - section.top)).toBe(8)
  // On the right: 2px, the 8px scrollbar, and less than one cell the fit leaves over.
  expect(section.right - screen.right).toBeGreaterThanOrEqual(9)
  expect(section.right - screen.right).toBeLessThan(19)
  expect(lastRow.bottom).toBeLessThanOrEqual(section.bottom - 7)
  expect(Math.round(bar.width)).toBe(8)
  expect(bar.left).toBeGreaterThanOrEqual(screen.right - 1)
  expect(bar.right).toBeLessThanOrEqual(section.right)
})

test('shows the scrollbar on hover, and keeps the pill off it (C18, C22)', async ({ page }) => {
  const shell = pane(page, 'Shell 1')
  await runIn(page, shell, 'seq 1 300')
  await expect(rows(shell)).toContainText('300')
  const bar = screenOf(shell).locator('.xterm-scrollable-element > .scrollbar.vertical')
  await page.mouse.move(0, 0)
  await expect(bar).toHaveClass(/invisible/, { timeout: 5_000 })

  await screenOf(shell).hover()
  await page.mouse.wheel(0, -2000)
  await expect(bar).toHaveClass(/(^|\s)visible/)
  const pill = shell.getByRole('button', { name: 'Jump to latest' })
  await expect(pill).toBeVisible()
  const pillBox = await pill.boundingBox()
  const barBox = await bar.boundingBox()
  expect((pillBox?.x ?? 0) + (pillBox?.width ?? 0)).toBeLessThanOrEqual(barBox?.x ?? 0)

  await pill.click()
  await expect(rows(shell)).toContainText('300')
})

test('dims the shells without the focus, and only them (C19)', async ({ page }) => {
  const first = pane(page, 'Shell 1')
  await screenOf(first).click()
  expect(await dimOf(first)).toBe('none')

  await addShells(page, 1)
  const second = pane(page, 'Shell 2')
  await screenOf(first).click()
  await expect.poll(() => dimOf(second)).toBe('0.3')
  expect(await dimOf(first)).toBe('none')

  // A click on the dimmed pane reaches its terminal and moves the dim.
  await screenOf(second).click()
  await expect.poll(() => dimOf(first)).toBe('0.3')
  expect(await dimOf(second)).toBe('none')

  // A maximized pane is the only one on screen, so it is never dimmed.
  await second.getByRole('button', { name: 'Maximize Shell 2' }).click()
  await screenOf(second).click()
  expect(await dimOf(second)).toBe('none')
  await second.getByRole('button', { name: 'Restore Shell 2' }).click()

  // With the focus outside the panes, none is dimmed.
  await page.getByRole('button', { name: 'Account' }).focus()
  await expect.poll(() => dimOf(first)).toBe('none')
  expect(await dimOf(second)).toBe('none')
})

test('moves the dim with the focus, with no extra resize and no motion (C20)', async ({ page }) => {
  const resizes: string[] = []
  page.on('websocket', (socket) => {
    socket.on('framesent', (frame) => {
      if (typeof frame.payload === 'string' && frame.payload.includes('"resize"')) {
        resizes.push(frame.payload)
      }
    })
  })
  await addShells(page, 1)
  const first = pane(page, 'Shell 1')
  const second = pane(page, 'Shell 2')
  await screenOf(first).click()
  resizes.length = 0

  for (let step = 0; step < 10; step++) {
    await screenOf(step % 2 === 0 ? second : first).click()
  }
  // At most the size claim of each focus, as on main. The dim adds none.
  expect(resizes.length).toBeLessThanOrEqual(10)
  await expect.poll(() => dimOf(second)).toBe('0.3')
  expect(await dimOf(first)).toBe('none')
  // The layer has no transition: it comes and goes at once. With reduced motion the app-wide rule
  // in styles.css allows 80ms opacity fades, and a layer that is only created runs none.
  const transition = await second
    .locator('.hm-terminal')
    .evaluate((element) => getComputedStyle(element, '::after').transitionDuration)
  expect(transition).toBe('0s')
})

test('follows the app theme with the Night palette, as --terminal (C13)', async ({ page }) => {
  const shell = pane(page, 'Shell 1')
  for (const theme of ['Dark', 'Light'] as const) {
    await setTheme(page, theme)
    await expect
      .poll(() => shell.evaluate((element) => getComputedStyle(element).backgroundColor))
      .toBe(BACKGROUND[theme])
  }
})

test('loads the icon font once, only when an icon shows (C8, C9)', async ({ page }) => {
  const iconRequests: string[] = []
  const errors: string[] = []
  page.on('request', (request) => {
    if (request.url().includes('SymbolsNerdFontMono')) {
      iconRequests.push(request.url())
    }
  })
  page.on('pageerror', (error) => errors.push(error.message))
  expect(await faceStatus(page, ICON_FONT)).toBe('unloaded')
  await addShells(page, 1)
  expect(iconRequests).toEqual([])

  // U+E0B0, the Powerline arrow, as UTF-8 bytes, so any printf prints it. Then a letter.
  for (const name of ['Shell 1', 'Shell 2']) {
    await runIn(page, pane(page, name), "printf 'icon:\\xee\\x82\\xb0X\\n'")
    await expect(rows(pane(page, name))).toContainText('icon:X')
  }
  await expect.poll(() => faceStatus(page, ICON_FONT)).toBe('loaded')
  const ready = await page.evaluate(
    (name) => document.fonts.check(`13px "${name}"`, ''),
    ICON_FONT,
  )
  expect(ready).toBe(true)
  expect(iconRequests).toHaveLength(1)
  expect(errors).toEqual([])
})
