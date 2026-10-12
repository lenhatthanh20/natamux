import type { Stage } from './stage.js'

// The stages a project can pick from in Project settings. The default pipeline is made of some of
// them. This file imports only types, so `stage.ts` can build the default pipeline from it.

export const STAGE_PHASES = ['understand', 'design', 'build', 'check', 'ship'] as const
export type StagePhase = (typeof STAGE_PHASES)[number]

export const PHASE_NAMES: Record<StagePhase, string> = {
  understand: 'Understand',
  design: 'Design',
  build: 'Build',
  check: 'Check',
  ship: 'Ship',
}

export type CatalogStage = {
  // What a pipeline holds when the stage is added: kind, gate and the fields of its checks.
  stage: Stage
  phase: StagePhase
  // One sentence for the picker.
  description: string
  // The default prompt of an agent or testing stage. `{commit}` becomes the commit rule.
  prompt?: string
  // A testing stage that only makes sense with the project's own command.
  needsCommand?: boolean
}

const READ_ONLY = 'Do not change any code.'

function prompt(...lines: string[]): string {
  return ['Task: {title}', ...lines].join('\n')
}

export const STAGE_CATALOG: readonly CatalogStage[] = [
  {
    phase: 'understand',
    stage: { id: 'context', kind: 'context', gate: 'manual', output: 'context.md' },
    description: 'Reads the context sources of the task into context.md.',
  },
  {
    phase: 'understand',
    stage: {
      id: 'research',
      kind: 'agent',
      gate: 'manual',
      output: 'research.md',
      sections: ['Related files', 'How it works today', 'Risks'],
    },
    description: 'Reads the code and docs the task touches, and lists what it found.',
    prompt: prompt(
      'Read {dir}/context.md. Then read the code and the docs that this task touches.',
      'Write {dir}/research.md with these sections: Related files, How it works today, Risks.',
      READ_ONLY,
    ),
  },
  {
    phase: 'understand',
    stage: {
      id: 'clarify',
      name: 'Open questions',
      kind: 'agent',
      gate: 'manual',
      output: 'questions.md',
      sections: ['Questions'],
    },
    description: 'Lists what is unclear in the task, with a suggested answer for each.',
    prompt: prompt(
      'Read {dir}/context.md.',
      'Write {dir}/questions.md with a Questions section: each thing that is unclear in the task, with a suggested answer.',
      READ_ONLY,
    ),
  },
  {
    phase: 'design',
    stage: {
      id: 'spec',
      kind: 'agent',
      gate: 'manual',
      output: 'spec.md',
      sections: ['Goal', 'Scope', 'Out of scope', 'Acceptance criteria', 'Code map'],
    },
    description:
      'Writes the goal, the scope, the acceptance criteria and a map of the code to change.',
    // The code map spares the next stages from reading the code again.
    prompt: prompt(
      'Read {dir}/context.md. Write {dir}/spec.md with these sections: Goal, Scope, Out of scope, Acceptance criteria, Code map.',
      'In Code map, list each file and function the change touches, with one line on why.',
      READ_ONLY,
    ),
  },
  {
    phase: 'design',
    stage: {
      id: 'ux-design',
      name: 'UX design',
      kind: 'agent',
      gate: 'manual',
      output: 'ux.md',
      sections: ['Screens', 'States', 'Flows', 'Accessibility'],
    },
    description: 'Describes the screens, their states and flows, and keyboard and accessibility.',
    prompt: prompt(
      'Read {dir}/context.md and {dir}/spec.md when it is there.',
      'Write {dir}/ux.md with these sections: Screens, States (empty, loading, error), Flows, Accessibility.',
      READ_ONLY,
    ),
  },
  {
    phase: 'design',
    stage: {
      id: 'architecture',
      kind: 'agent',
      gate: 'manual',
      output: 'architecture.md',
      sections: ['Components', 'Data flow', 'Interfaces', 'Trade-offs'],
    },
    description: 'Describes the components, the data flow, the interfaces and the trade-offs.',
    prompt: prompt(
      'Read {dir}/context.md and {dir}/spec.md when it is there, then the code it touches.',
      'Write {dir}/architecture.md with these sections: Components, Data flow, Interfaces, Trade-offs.',
      READ_ONLY,
    ),
  },
  {
    phase: 'design',
    stage: {
      id: 'threat-model',
      kind: 'agent',
      gate: 'manual',
      output: 'threat-model.md',
      sections: ['Assets', 'Entry points', 'Threats'],
    },
    description: 'Lists what an attacker could reach, and how each threat is handled.',
    prompt: prompt(
      'Read {dir}/context.md and {dir}/spec.md when it is there, then the code it touches.',
      'Write {dir}/threat-model.md with these sections: Assets, Entry points, Threats (each with how it is handled).',
      READ_ONLY,
    ),
  },
  {
    phase: 'design',
    stage: {
      id: 'plan',
      kind: 'agent',
      gate: 'manual',
      output: 'plan.md',
      sections: ['Steps', 'Files', 'Tests'],
      workflow: true,
    },
    description: 'Writes the steps, the files to change and the tests to add.',
    prompt: prompt(
      'Read {dir}/context.md and {dir}/spec.md. Write {dir}/plan.md with these sections: Steps, Files, Tests.',
      READ_ONLY,
    ),
  },
  {
    phase: 'build',
    stage: {
      id: 'test-spec',
      kind: 'agent',
      gate: 'manual',
      output: 'test-spec.md',
      sections: ['Cases'],
      ownShell: true,
    },
    description: 'Writes test cases from the spec, without reading the code.',
    prompt: prompt(
      'Read {dir}/context.md and {dir}/spec.md. Do not read the code.',
      'Write {dir}/test-spec.md with a Cases section: one case per item, each with its input, the expected result and the edge cases.',
      READ_ONLY,
    ),
  },
  {
    phase: 'build',
    // No commit check: the agent leaves its changes for you to read before you commit them.
    stage: { id: 'implement', kind: 'agent', gate: 'auto', workflow: true, ci: true },
    description: 'Writes the tests first, then the code that makes them pass.',
    prompt: prompt(
      'Read {dir}/spec.md and {dir}/plan.md when they are there.',
      'Write the tests first, then the code that makes them pass. {commit}',
    ),
  },
  {
    phase: 'build',
    stage: { id: 'migration', name: 'Data migration', kind: 'agent', gate: 'manual' },
    description: 'Writes the data or schema migration, and how to roll it back.',
    prompt: prompt(
      'Read {dir}/spec.md and {dir}/plan.md when they are there.',
      'Write the data or schema migration this task needs, with a way to roll it back, and a test for both. {commit}',
    ),
  },
  {
    phase: 'build',
    stage: { id: 'refactor', kind: 'agent', gate: 'auto' },
    description: 'Cleans up the changed code without changing what it does.',
    prompt: prompt(
      'Look at the changes of this task. Clean them up: names, duplicate code, long functions.',
      'Do not change what the code does. The tests must still pass. {commit}',
    ),
  },
  {
    phase: 'build',
    stage: { id: 'docs', name: 'Documentation', kind: 'agent', gate: 'auto' },
    description: 'Updates the user and developer docs for the change.',
    prompt: prompt(
      'Look at the changes of this task. Update the user docs and the developer docs that they make out of date. {commit}',
    ),
  },
  {
    phase: 'build',
    stage: { id: 'i18n', name: 'Translations', kind: 'agent', gate: 'auto' },
    description: 'Adds or updates the translations of new UI text.',
    prompt: prompt(
      'Look at the changes of this task. Add or update the translations of each new or changed UI text. {commit}',
    ),
  },
  {
    phase: 'check',
    stage: { id: 'lint', name: 'Lint and types', kind: 'testing', gate: 'auto' },
    description: "Runs the project's lint and type check command.",
    needsCommand: true,
  },
  {
    phase: 'check',
    stage: {
      id: 'self-review',
      name: 'Agent self-review',
      kind: 'agent',
      gate: 'auto',
      output: 'self-review.md',
      sections: ['Problems'],
    },
    description: 'Reads the diff against the spec, lists the problems and fixes them.',
    prompt: prompt(
      'Read {dir}/spec.md when it is there. Read the diff of this task against it.',
      'Write {dir}/self-review.md with a Problems section, then fix each problem. {commit}',
    ),
  },
  {
    phase: 'check',
    stage: {
      id: 'security',
      name: 'Security review',
      kind: 'agent',
      gate: 'manual',
      output: 'security.md',
      sections: ['Findings'],
      ownShell: true,
    },
    description: 'Checks the diff for security problems.',
    prompt: prompt(
      'Read the diff of this task. Look for security problems: input that is not checked, secrets, injection, access checks.',
      'Write {dir}/security.md with a Findings section: each problem, where it is, and how to fix it.',
      READ_ONLY,
    ),
  },
  {
    phase: 'check',
    stage: {
      id: 'performance',
      name: 'Performance check',
      kind: 'agent',
      gate: 'manual',
      output: 'performance.md',
      sections: ['Findings'],
      ownShell: true,
    },
    description: 'Looks for slow paths in the diff, and measures them when it can.',
    prompt: prompt(
      'Read the diff of this task. Look for slow paths: loops over large data, extra requests, work done twice. Measure when you can.',
      'Write {dir}/performance.md with a Findings section.',
      READ_ONLY,
    ),
  },
  {
    phase: 'check',
    stage: {
      id: 'accessibility',
      name: 'Accessibility check',
      kind: 'agent',
      gate: 'manual',
      output: 'accessibility.md',
      sections: ['Findings'],
      ownShell: true,
    },
    description: 'Checks UI changes for keyboard, screen reader and contrast problems.',
    prompt: prompt(
      'Read the UI changes of this task. Check keyboard use, screen reader names and roles, focus, and color contrast.',
      'Write {dir}/accessibility.md with a Findings section.',
      READ_ONLY,
    ),
  },
  {
    phase: 'check',
    stage: {
      id: 'agent-review',
      name: 'Review',
      kind: 'agent',
      gate: 'auto',
      output: 'review.md',
      sections: ['Findings'],
      ownShell: true,
    },
    description:
      'An agent reviews the changes against the spec, and sends the task back to Implement when they need fixes.',
    // The agent sends the task back itself, so no person has to read the report first.
    prompt: prompt(
      'Read {dir}/spec.md, and {dir}/test-spec.md when it is there. Review the changes of this task against them: what is not committed (git status, git diff) and the commits of this branch that are not on the default branch (git log, and git diff from the merge base). Read the changed files. Do not change any code.',
      'Write {dir}/review.md with a Findings section: each problem that must be fixed, with its file and why, or "None".',
      'When a problem must be fixed, run `natamux stage back $NATAMUX_TASK_ID --reason "<the problems, one per line>"` and stop. Do not run natamux done then.',
    ),
  },
  {
    phase: 'check',
    stage: { id: 'review', name: 'Manual review', kind: 'review', gate: 'manual' },
    description: 'A person reads the diff, comments, and approves it or sends it back.',
  },
  {
    phase: 'check',
    stage: { id: 'testing', name: 'Automated tests', kind: 'testing', gate: 'auto', ci: true },
    description:
      'An agent adds a test for each case of the test spec and runs all tests, or a command runs them.',
    prompt: prompt(
      'For each case in {dir}/test-spec.md that has no test yet, add one. Then run all the tests and fix what fails. {commit}',
    ),
  },
  {
    phase: 'check',
    stage: { id: 'e2e', name: 'End-to-end tests', kind: 'testing', gate: 'auto' },
    description: 'Adds end-to-end tests for the change, then runs them.',
    prompt: prompt(
      'Add end-to-end tests for what this task changes, where the project has end-to-end tests. {commit}',
    ),
    needsCommand: true,
  },
  {
    phase: 'check',
    stage: { id: 'qa', name: 'Manual testing', kind: 'manual', gate: 'manual' },
    description: 'A person tries the change by hand.',
  },
  {
    phase: 'check',
    stage: { id: 'design-review', kind: 'manual', gate: 'manual' },
    description: 'A designer checks the UI.',
  },
  {
    phase: 'ship',
    stage: { id: 'changelog', name: 'Release notes', kind: 'agent', gate: 'auto' },
    description: 'Adds a changelog entry for the change.',
    prompt: prompt(
      'Look at the changes of this task. Add an entry for them to the changelog of the project. {commit}',
    ),
  },
  {
    phase: 'ship',
    stage: { id: 'staging', name: 'Staging check', kind: 'manual', gate: 'manual' },
    description: 'You check the change on a staging server.',
  },
  {
    phase: 'ship',
    stage: { id: 'release', kind: 'release', gate: 'manual' },
    description: 'Pushes the branch and opens a pull request. Ends when it is merged.',
  },
]

export function catalogEntry(id: string): CatalogStage | undefined {
  return STAGE_CATALOG.find((entry) => entry.stage.id === id)
}
