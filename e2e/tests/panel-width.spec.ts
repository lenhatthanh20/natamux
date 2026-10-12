import { expect, type Locator, type Page, test } from '@playwright/test'
import { DEMO_TITLE, signIn, taskNamed, taskTable } from './helpers'

const SIDEBAR_KEY = 'natamux.sidebar-width'
const DETAILS_KEY = 'natamux.details-width'

function sidebar(page: Page): Locator {
  return page.getByRole('navigation', { name: 'Workspaces' })
}

function details(page: Page): Locator {
  return page.getByRole('complementary', { name: /^Details of / })
}

function sidebarHandle(page: Page): Locator {
  return page.getByRole('separator', { name: 'Resize the sidebar' })
}

function detailsHandle(page: Page): Locator {
  return page.getByRole('separator', { name: 'Resize the details' })
}

async function widthOf(locator: Locator): Promise<number> {
  return locator.evaluate((element) => element.getBoundingClientRect().width)
}

async function stored(page: Page, key: string): Promise<string | null> {
  return page.evaluate((name) => localStorage.getItem(name), key)
}

// A pick in the table shows the task's details, docked where there is room.
async function showDetails(page: Page): Promise<void> {
  await page.getByRole('radio', { name: 'Table' }).click()
  await taskTable(page)
    .getByRole('button', { name: taskNamed(DEMO_TITLE) })
    .click()
  await expect(details(page)).toBeVisible()
}

// Drags a handle sideways by dx pixels, as a user does with the mouse.
async function drag(page: Page, handle: Locator, dx: number): Promise<void> {
  const box = await handle.boundingBox()
  if (box === null) {
    throw new Error('The handle is not on screen')
  }
  const x = box.x + box.width / 2
  const y = box.y + box.height / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x + dx, y, { steps: 5 })
  await page.mouse.up()
}

async function saveWidths(page: Page, widths: Record<string, string>): Promise<void> {
  await page.evaluate((entries) => {
    for (const [key, value] of Object.entries(entries)) {
      localStorage.setItem(key, value)
    }
  }, widths)
  await page.reload()
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
}

test.describe('default widths', () => {
  test('docks both panels at 320 at 1440', async ({ page }) => {
    await signIn(page)
    // Details closed: the sidebar has its default already.
    expect(await widthOf(sidebar(page))).toBe(320)

    await showDetails(page)
    const left = await widthOf(sidebar(page))
    const right = await widthOf(details(page))
    expect(left).toBe(320)
    expect(right).toBe(left)
    // The two 7px handles overlap the panels and take 1px each.
    const main = await widthOf(page.locator('#main'))
    expect(main).toBeGreaterThanOrEqual(1440 - 640 - 2)
    expect(main).toBeLessThanOrEqual(1440 - 640)
  })

  test('keeps both panels at 320 in a wider window', async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 })
    await signIn(page)
    await showDetails(page)
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await widthOf(details(page))).toBe(320)
  })

  test('keeps the details docked at 1280, where main is just 640', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await signIn(page)
    await showDetails(page)
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await widthOf(details(page))).toBe(320)
    await expect(page.getByRole('dialog', { name: 'Details' })).toHaveCount(0)
    await expect(detailsHandle(page)).toBeVisible()
  })

  test('caps the sidebar at 30% of the window at 1024', async ({ page }) => {
    await page.setViewportSize({ width: 1024, height: 768 })
    await signIn(page)
    expect(await widthOf(sidebar(page))).toBe(Math.round(1024 * 0.3))
  })
})

test.describe('reset', () => {
  // 1920 leaves room to dock 400 + 500.
  test.use({ viewport: { width: 1920, height: 1080 } })

  test('a double click or Enter puts each panel back to 320', async ({ page }) => {
    await signIn(page)
    await saveWidths(page, { [SIDEBAR_KEY]: '400', [DETAILS_KEY]: '500' })
    await showDetails(page)
    expect(await widthOf(sidebar(page))).toBe(400)
    expect(await widthOf(details(page))).toBe(500)

    await sidebarHandle(page).dblclick()
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await widthOf(details(page))).toBe(500)
    expect(await stored(page, SIDEBAR_KEY)).toBe('320')

    await detailsHandle(page).focus()
    await page.keyboard.press('Enter')
    expect(await widthOf(details(page))).toBe(320)
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await stored(page, DETAILS_KEY)).toBe('320')

    // Already at 320: nothing changes.
    await sidebarHandle(page).focus()
    await page.keyboard.press('Enter')
    expect(await widthOf(sidebar(page))).toBe(320)

    await page.reload()
    await showDetails(page)
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await widthOf(details(page))).toBe(320)
  })

  test('resets the sidebar from its min and its max', async ({ page }) => {
    await signIn(page)
    for (const saved of ['220', '440']) {
      await saveWidths(page, { [SIDEBAR_KEY]: saved })
      expect(await widthOf(sidebar(page))).toBe(Number(saved))
      await sidebarHandle(page).dblclick()
      expect(await widthOf(sidebar(page))).toBe(320)
    }
  })
})

test.describe('saved widths', () => {
  test('loads the old defaults that a user saved', async ({ page }) => {
    await signIn(page)
    await saveWidths(page, { [SIDEBAR_KEY]: '272', [DETAILS_KEY]: '360' })
    await showDetails(page)
    expect(await widthOf(sidebar(page))).toBe(272)
    expect(await widthOf(details(page))).toBe(360)
  })

  test('uses 320 for the panel with no saved width', async ({ page }) => {
    await signIn(page)
    await saveWidths(page, { [DETAILS_KEY]: '400' })
    await showDetails(page)
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await widthOf(details(page))).toBe(400)
  })

  // A saved width outside the limits is not trusted: the panel starts at its default.
  for (const saved of ['abc', '', '-5', '100', '9999']) {
    test(`starts at 320 when the saved width is "${saved}"`, async ({ page }) => {
      await signIn(page)
      await saveWidths(page, { [SIDEBAR_KEY]: saved, [DETAILS_KEY]: saved })
      await showDetails(page)
      expect(await widthOf(sidebar(page))).toBe(320)
      expect(await widthOf(details(page))).toBe(320)
    })
  }
})

test.describe('drag', () => {
  test('dragging one handle changes only that panel', async ({ page }) => {
    await signIn(page)
    await showDetails(page)

    await drag(page, sidebarHandle(page), 60)
    expect(await widthOf(sidebar(page))).toBe(380)
    expect(await widthOf(details(page))).toBe(320)
    expect(await stored(page, SIDEBAR_KEY)).toBe('380')
    expect(await stored(page, DETAILS_KEY)).toBeNull()

    // The details grow to the left, and stop at their min to the right.
    await drag(page, detailsHandle(page), 50)
    expect(await widthOf(details(page))).toBe(320)
    expect(await widthOf(sidebar(page))).toBe(380)
  })

  test('dragging the details changes only the details', async ({ page }) => {
    await signIn(page)
    await showDetails(page)

    await drag(page, detailsHandle(page), -100)
    expect(await widthOf(details(page))).toBe(420)
    expect(await widthOf(sidebar(page))).toBe(320)
    expect(await stored(page, SIDEBAR_KEY)).toBeNull()

    await detailsHandle(page).focus()
    await page.keyboard.press('ArrowRight')
    expect(await widthOf(details(page))).toBe(404)
    expect(await widthOf(sidebar(page))).toBe(320)
  })
})
