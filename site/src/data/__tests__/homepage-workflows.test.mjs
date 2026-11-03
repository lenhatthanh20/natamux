import assert from 'node:assert/strict'
import test from 'node:test'
import { workflowExamples } from '../homepage-workflows.mjs'

const walk = (rows) => rows.flatMap((row) => [
  row,
  ...walk(row.par ?? []),
  ...(row.paths ?? []).flatMap((path) => walk(path.rows)),
])

test('the homepage shows distinct solo and enterprise workflow examples', () => {
  assert.deepEqual(workflowExamples.map(({ id }) => id), ['solo', 'enterprise'])

  const [solo, enterprise] = workflowExamples
  const soloSteps = walk(solo.rows)
  const enterpriseSteps = walk(enterprise.rows)

  assert.equal(soloSteps.filter((step) => step.you).length, 1)
  assert.ok(soloSteps.some((step) => step.par?.length >= 2))
  assert.ok(enterpriseSteps.some((step) => step.paths?.length >= 2))
  assert.ok(enterpriseSteps.filter((step) => step.you).length >= 2)
  assert.ok(enterpriseSteps.some((step) => /CI/.test(step.n ?? '')))
})

test('each example points to a related workflow file', () => {
  for (const example of workflowExamples) {
    assert.ok(example.name)
    assert.ok(example.text)
    assert.match(example.anchor, /^the-(solo|team)-file$/)
  }
})
