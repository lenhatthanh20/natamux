import { expect, test } from '@playwright/test'
import { signIn, taskNamed, taskTable } from './helpers'

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test('ticks Run with a workflow on a stage of a new task', async ({ page }) => {
  const title = `Workflow task of the e2e test ${Date.now()}`
  await page.getByRole('radio', { name: 'Table' }).click()
  await page.getByRole('textbox', { name: 'New task title' }).fill(title)
  await page.getByRole('button', { name: 'Add task', exact: true }).click()
  await taskTable(page)
    .getByRole('button', { name: taskNamed(title) })
    .click()

  const details = page.getByRole('complementary', { name: /^Details of T-/ })
  // The panel opens on the view picked last, which may be Details.
  await details.getByRole('tab', { name: 'Progress' }).click()
  const stages = details.getByRole('list', { name: 'Stages' })
  const implement = stages.getByRole('article', { name: 'Implement' })
  const box = implement.getByRole('checkbox', { name: 'Run with a workflow' })
  await expect(box).not.toBeChecked()
  await expect(implement.getByText('More accurate, uses many more tokens.')).toBeVisible()
  await expect(
    stages.getByRole('article', { name: 'Test spec' }).getByRole('checkbox'),
  ).toHaveCount(0)

  // The box shows the task the daemon sends back, so a tick means the choice was saved.
  await box.click()
  await expect(box).toBeChecked()
  await box.click()
  await expect(box).not.toBeChecked()
})

test('offers a workflow on Plan in the project settings', async ({ page }) => {
  const sidebar = page.getByRole('navigation', { name: 'Workspaces' })
  await sidebar.getByRole('button', { name: 'Add a project' }).click()
  const projects = page.getByRole('dialog', { name: 'Projects' })
  await projects.getByRole('button', { name: 'Settings' }).first().click()

  const settings = page.getByRole('dialog', { name: /^Project settings of / })
  const stages = settings.getByRole('list', { name: 'Stages' })
  await stages.getByRole('button', { name: /^Plan\b/ }).click()
  const plan = settings.getByRole('region', { name: 'Settings of Plan' })
  const box = plan.getByRole('checkbox', { name: 'Tasks can run it as a workflow' })
  await expect(box).toBeChecked()

  await stages.getByRole('button', { name: /^Spec\b/ }).click()
  const spec = settings.getByRole('region', { name: 'Settings of Spec' })
  await spec.getByRole('checkbox', { name: 'Tasks can run it as a workflow' }).check()
  await settings.getByRole('tab', { name: 'JSON' }).click()
  await expect(settings.getByRole('textbox', { name: '.natamux.json' })).toHaveValue(
    /"id": "spec",[^}]*"workflow": true/,
  )
})
