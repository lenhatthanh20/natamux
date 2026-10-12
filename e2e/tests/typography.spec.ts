import { expect, test } from '@playwright/test'
import { signIn } from './helpers'

// UI text is Inter at 13px, the UI font of JetBrains IDEs. Code keeps its mono fonts.
test('draws UI text in Inter at 13px, and code in mono fonts', async ({ page }) => {
  await signIn(page)
  const body = await page.evaluate(() => {
    const style = getComputedStyle(document.body)
    return { family: style.fontFamily, size: style.fontSize }
  })
  expect(body.family.startsWith('"Inter Variable"')).toBe(true)
  expect(body.size).toBe('13px')
  await expect
    .poll(() => page.evaluate(() => document.fonts.check('13px "Inter Variable"', 'Tiếng Việt')))
    .toBe(true)

  const mono = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim(),
  )
  expect(mono.startsWith('"Geist Mono Variable"')).toBe(true)
})
