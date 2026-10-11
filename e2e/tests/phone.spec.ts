import { expect, test } from '@playwright/test'
import { signIn, taskNamed } from './helpers'

test('lists the tasks on a phone, with no sideways scrolling', async ({ page }) => {
  await signIn(page)
  await expect(
    page.getByRole('button', { name: taskNamed('Fix token refresh race in the daemon') }).first(),
  ).toBeVisible()
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  )
  expect(overflow).toBeLessThanOrEqual(0)
})
