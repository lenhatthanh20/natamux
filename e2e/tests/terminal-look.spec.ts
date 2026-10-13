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

// The outline style of a pane: 'solid' while it has the focus, 'none' without.
async function outlineOf(shell: Locator): Promise<string> {
  return shell.evaluate((element) => getComputedStyle(element).outlineStyle)
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

test('does not dim the shells without the focus, and outlines the focused one (C19)', async ({
  page,
}) => {
  const regions = page.getByRole('region', { name: /^Shell \d+$/ })
  const start = await regions.count()
  await addShells(page, 2)
  const shells = [pane(page, 'Shell 1'), pane(page, 'Shell 2'), pane(page, 'Shell 3')]
  const [first, second] = shells

  for (const focused of shells) {
    await screenOf(focused).click()
    await expect.poll(() => outlineOf(focused)).toBe('solid')
    for (const shell of shells) {
      // Read at once after the focus move, so a fade left over would show.
      expect(await dimOf(shell)).toBe('none')
      if (shell !== focused) expect(await outlineOf(shell)).toBe('none')
    }
  }

  // A click reaches the pane's terminal, so the keys go there and nowhere else.
  await runIn(page, second, 'echo T0032B')
  await expect(rows(second)).toContainText('T0032B')
  await expect(rows(first)).not.toContainText('T0032B')
  await runIn(page, first, 'echo T0032A')
  await expect(rows(first)).toContainText('T0032A')
  await expect(rows(second)).not.toContainText('T0032A')

  // A pane added while another one has the focus is not dimmed either.
  await screenOf(first).click()
  const before = await regions.count()
  await addShells(page, 1)
  expect(await dimOf(pane(page, `Shell ${before + 1}`))).toBe('none')

  // With the focus outside the panes, none has the outline.
  await page.getByRole('button', { name: 'Account' }).focus()
  for (const shell of shells) {
    await expect.poll(() => outlineOf(shell)).toBe('none')
    expect(await dimOf(shell)).toBe('none')
  }
  await expect(page.locator('.hm-pane, .hm-panes')).toHaveCount(0)

  // The shells stay in the task: close the ones added here, so the next tests can add theirs
  // under the limit of panes.
  for (let index = await regions.count(); index > start; index--) {
    await page.getByRole('button', { name: `Close Shell ${index}` }).click()
    await expect(regions).toHaveCount(index - 1)
  }
})

test('does not dim the agents without the focus, and outlines the focused one', async ({
  page,
}) => {
  await page.getByRole('button', { name: /^Agents( \d+)?$/ }).click()
  const agents = page.getByRole('region', { name: 'Agents', exact: true })
  const panes = agents.getByRole('region', { name: /^Agent of / })
  const add = agents.getByRole('combobox', { name: 'Add a pane' })
  // A fresh daemon may have no agent pane yet: add tasks until two show.
  await expect(add.or(panes.nth(1)).first()).toBeVisible()
  while ((await panes.count()) < 2) {
    await add.click()
    await page.getByRole('option').first().click()
  }
  const shown = [panes.nth(0), panes.nth(1)]

  for (const focused of shown) {
    await screenOf(focused).click()
    await expect(focused.locator('.xterm-helper-textarea')).toBeFocused()
    await expect.poll(() => outlineOf(focused)).toBe('solid')
    for (const agent of shown) {
      expect(await dimOf(agent)).toBe('none')
      if (agent !== focused) expect(await outlineOf(agent)).toBe('none')
    }
  }

  await page.getByRole('button', { name: 'Account' }).focus()
  for (const agent of shown) {
    await expect.poll(() => outlineOf(agent)).toBe('none')
    expect(await dimOf(agent)).toBe('none')
  }
})

test('moves the focus between panes with no extra resize (C20)', async ({ page }) => {
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
  // At most the size claim of each focus, as on main.
  expect(resizes.length).toBeLessThanOrEqual(10)
  await expect.poll(() => outlineOf(first)).toBe('solid')
  expect(await dimOf(second)).toBe('none')
})

test('keeps an ended shell dimmed, focused or not, with no dim on top', async ({ page }) => {
  const before = await page.getByRole('region', { name: /^Shell \d+$/ }).count()
  await addShells(page, 1)
  const running = pane(page, 'Shell 1')
  const ended = pane(page, `Shell ${before + 1}`)
  await runIn(page, ended, 'exit')
  await expect(ended.getByRole('button', { name: 'Open a new shell' })).toBeVisible()

  await screenOf(running).click()
  await expect(screenOf(ended)).toHaveCSS('opacity', '0.6')
  await expect(screenOf(running)).toHaveCSS('opacity', '1')
  expect(await dimOf(ended)).toBe('none')
  expect(await dimOf(running)).toBe('none')

  await screenOf(ended).click()
  await expect(screenOf(ended)).toHaveCSS('opacity', '0.6')
  await expect(screenOf(running)).toHaveCSS('opacity', '1')
  expect(await dimOf(ended)).toBe('none')
  expect(await dimOf(running)).toBe('none')
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

test('shows code in the Changes tab with the font of the terminal', async ({ page }) => {
  await page
    .getByRole('tablist', { name: 'View', exact: true })
    .getByRole('tab', { name: 'Changes' })
    .click()
  const lines = page.locator('.monaco-editor .view-lines').first()
  await expect(lines).toBeVisible({ timeout: 30_000 })
  const font = await lines.evaluate((element) => getComputedStyle(element).fontFamily)
  expect(font.startsWith(`"${MAIN_FONT}"`)).toBe(true)
})
