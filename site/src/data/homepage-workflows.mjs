// These examples share the same stage settings. The cards show patterns, not product modes.
export const workflowExamples = [
  {
    id: 'solo',
    name: 'Solo',
    audience: 'One person, one repository',
    text: 'Approve the spec, then let agents build, review and watch CI. Change any step when the work calls for it.',
    anchor: 'the-solo-file',
    rows: [
      { n: 'Context' },
      { n: 'Spec', you: 'You approve' },
      { par: [{ n: 'Test spec', a: 'copilot' }, { n: 'Implement', a: 'claude' }] },
      { n: 'Review', a: 'cursor', back: 'Sends back to Implement' },
      { n: 'Watch CI and merge', a: 'copilot', back: 'Sends back on red CI' },
    ],
  },
  {
    id: 'enterprise',
    name: 'Enterprise workflow',
    audience: 'More tasks, more review points',
    text: 'Route each task through the steps it needs. Add people, agents and checks where your team wants them.',
    anchor: 'the-team-file',
    rows: [
      { n: 'Context' },
      { n: 'Triage', you: 'You approve' },
      {
        branch: 'Where is the task',
        paths: [
          {
            name: "Review a teammate's PR",
            rows: [
              { n: 'Review PR', a: 'claude' },
              { n: 'Follow PR CI' },
              { n: 'Wait for fix' },
              { n: 'QA build' },
            ],
          },
          {
            name: 'Develop',
            other: true,
            rows: [
              { n: 'Spec', you: 'You approve' },
              { n: 'Plan' },
              { n: 'Implement' },
              { n: 'Review', a: 'claude', back: 'Sends back to Implement' },
              { n: 'Open PR' },
              { n: 'Follow CI', a: 'cursor', back: 'Sends back on red CI' },
              { n: 'Definition of done', a: 'copilot' },
            ],
          },
        ],
      },
      { n: 'Final check', you: 'You check' },
    ],
  },
]
