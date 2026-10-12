import { expect, type Locator, type Page, test } from '@playwright/test'
import { DEMO_TITLE, openTask, signIn } from './helpers'
import { addShells, pane, rows, runIn } from './terminal-helpers'

const ICON_FONT = 'Symbols Nerd Font Mono'

test.beforeEach(async ({ page }) => {
  await signIn(page)
  const tabs = await openTask(page, DEMO_TITLE)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Shell 1'))).toContainText('$', { timeout: 30_000 })
})

// The dim layer of a pane's screen: its opacity, or 'none' when the pane is not dimmed.
async function dimOf(shell: Locator): Promise<string> {
  return shell.locator('.hm-terminal').evaluate((element) => {
    const after = getComputedStyle(element, '::after')
    return after.content === 'none' ? 'none' : after.opacity
  })
}

async function iconFontStatus(page: Page): Promise<string> {
  return page.evaluate((family) => {
    let status = 'missing'
    document.fonts.forEach((face) => {
      if (face.family.includes(family)) {
        status = face.status
      }
    })
    return status
  }, ICON_FONT)
}

test('dims the shells without the focus, and only them', async ({ page }) => {
  await addShells(page, 1)
  const first = pane(page, 'Shell 1')
  const second = pane(page, 'Shell 2')
  await first.getByRole('region', { name: 'Terminal screen' }).click()
  await expect.poll(() => dimOf(second)).toBe('0.3')
  expect(await dimOf(first)).toBe('none')

  // A click on the dimmed pane reaches its terminal and moves the focus.
  await second.getByRole('region', { name: 'Terminal screen' }).click()
  await expect.poll(() => dimOf(first)).toBe('0.3')
  expect(await dimOf(second)).toBe('none')
})

test('loads the icon font only when an icon shows, and draws icons with it', async ({ page }) => {
  expect(await iconFontStatus(page)).toBe('unloaded')

  // U+E0B0, the Powerline arrow, as UTF-8 bytes, so any printf prints it.
  await runIn(page, pane(page, 'Shell 1'), "printf 'icon:\\xee\\x82\\xb0\\n'")
  await expect(rows(pane(page, 'Shell 1'))).toContainText('icon:\ue0b0')
  await expect.poll(() => iconFontStatus(page)).toBe('loaded')
})
