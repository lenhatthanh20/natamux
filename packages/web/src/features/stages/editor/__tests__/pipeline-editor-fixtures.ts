import { type Step, stepRefsOf } from '@natamux/core'

export const ids = (steps: readonly Step[]) => stepRefsOf(steps)

// Context, Plan, a branch of two paths, Release.
export const BRANCHED: Step[] = [
  { id: 'context', kind: 'context', gate: 'manual' },
  { id: 'plan', kind: 'agent', gate: 'manual' },
  {
    id: 'after-plan',
    name: 'After plan',
    paths: [
      {
        id: 'small',
        name: 'Small change',
        when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
        stages: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'testing', kind: 'testing', gate: 'auto' },
        ],
      },
      {
        id: 'pr-review',
        name: 'Review PR',
        when: { type: 'command', run: 'test -f .no-review' },
        not: true,
        stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
      },
    ],
  },
  { id: 'release', kind: 'release', gate: 'manual' },
]

export const branchRef = (small: string[], review: string[]) => ({
  id: 'after-plan',
  paths: [
    { id: 'small', stages: small },
    { id: 'pr-review', stages: review },
  ],
})
