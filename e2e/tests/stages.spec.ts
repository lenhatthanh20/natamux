import { type APIRequestContext, expect, test } from '@playwright/test'
import { signIn, TOKEN } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }

type Stage = { id: string; gate: string; prompt?: string; output?: string }
type Step = Stage | { id: string; parallel: Stage[] }

async function firstRepo(request: APIRequestContext): Promise<{ id: string; name: string }> {
  const body = await (await request.get('/api/repos', { headers })).json()
  return body.repos[0]
}

async function savedStages(request: APIRequestContext, repo: string): Promise<Stage[]> {
  const body = await (await request.get(`/api/repos/${repo}/settings`, { headers })).json()
  const steps: Step[] = body.settings.stages
  return steps.flatMap((step) => ('parallel' in step ? step.parallel : [step]))
}

const PROMPT = 'Task: {title}\nLook for secrets in the diff. Write {dir}/security.md with Findings.'

// The tests share one daemon, so the settings of the project are put back at the end.
test('changes the stages of a project in Project settings', async ({ page, request }) => {
  const repo = await firstRepo(request)
  const before = await (await request.get(`/api/repos/${repo.id}/settings`, { headers })).json()
  let taskId: string | undefined
  try {
    await signIn(page)
    await page
      .getByRole('navigation', { name: 'Workspaces' })
      .getByRole('button', { name: 'Add a project' })
      .click()
    const projects = page.getByRole('dialog', { name: 'Projects' })
    await projects
      .getByRole('listitem')
      .filter({ hasText: repo.name })
      .getByRole('button', { name: 'Settings' })
      .click()

    const dialog = page.getByRole('dialog', { name: `Project settings of ${repo.name}` })
    await expect(dialog.getByRole('tab', { name: 'Stages', selected: true })).toBeVisible()

    // A catalog stage, with a prompt of its own.
    await dialog.getByRole('button', { name: 'Add stage' }).click()
    await dialog.getByRole('textbox', { name: 'Search stages' }).fill('Security')
    await dialog.getByRole('button', { name: /^Security review/ }).click()
    const security = dialog.getByRole('region', { name: 'Settings of Security review' })
    await security.getByRole('textbox', { name: 'Prompt' }).fill(PROMPT)
    await expect(security.getByRole('group', { name: 'Prompt' }).getByText('Changed')).toBeVisible()
    await expect(
      dialog.getByRole('list', { name: /^Pipeline: .*then Security review, then Release$/ }),
    ).toBeVisible()

    // Spec moves on without waiting for approval.
    await dialog
      .getByRole('list', { name: 'Stages' })
      .getByRole('button', { name: /^Spec\b/ })
      .click()
    const spec = dialog.getByRole('region', { name: 'Settings of Spec' })
    await spec.getByRole('checkbox', { name: 'Wait for my approval' }).uncheck()
    await expect(spec.getByRole('button', { name: 'Use default' })).toBeVisible()

    await dialog.getByRole('button', { name: /^Save/ }).click()
    await expect(dialog).toHaveCount(0)

    const stages = await savedStages(request, repo.id)
    expect(stages.find((stage) => stage.id === 'security')).toMatchObject({
      gate: 'manual',
      output: 'security.md',
      prompt: PROMPT,
    })
    expect(stages.find((stage) => stage.id === 'spec')?.gate).toBe('auto')
    expect(stages.find((stage) => stage.id === 'plan')?.prompt).toBeUndefined()

    // A task made now gets the new stage.
    const created = await request.post('/api/tasks', {
      headers,
      data: { title: `Stages e2e ${Date.now()}`, repo: repo.id },
    })
    const { task } = await created.json()
    taskId = task.id
    expect(JSON.stringify(task.stages)).toContain('"security"')
  } finally {
    if (taskId !== undefined) {
      await request.delete(`/api/tasks/${taskId}`, { headers })
    }
    await request.put(`/api/repos/${repo.id}/settings`, {
      headers,
      data: before.text === null ? {} : JSON.parse(before.text),
    })
  }
})
