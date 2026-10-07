import { describe, expect, it } from 'vitest'
import { parseCommand, UsageError } from './command.js'

describe('parseCommand', () => {
  it.each([[[]], [['help']], [['task', 'list', '--help']], [['-h']]])(
    'shows help for %j',
    (argv) => {
      expect(parseCommand(argv)).toEqual({ name: 'help' })
    },
  )

  describe('task create', () => {
    it('joins the words of the title', () => {
      expect(parseCommand(['task', 'create', 'Fix', 'the', 'bug'])).toMatchObject({
        name: 'task.create',
        title: 'Fix the bug',
        labels: [],
        dependsOn: [],
        json: false,
      })
    })

    it('reads every option', () => {
      const command = parseCommand([
        'task',
        'create',
        'Fix the bug',
        '--description',
        'It is slow',
        '--priority',
        'high',
        '--label',
        'bug',
        '--label',
        'perf',
        '--agent',
        'claude-code',
        '--depends-on',
        'T-1',
        '--depends-on',
        'T-2',
        '--json',
      ])
      expect(command).toEqual({
        name: 'task.create',
        title: 'Fix the bug',
        description: 'It is slow',
        priority: 'high',
        labels: ['bug', 'perf'],
        agent: 'claude-code',
        dependsOn: ['T-1', 'T-2'],
        json: true,
      })
    })

    it('needs a title', () => {
      expect(() => parseCommand(['task', 'create'])).toThrow(UsageError)
    })

    it('rejects an unknown priority', () => {
      expect(() => parseCommand(['task', 'create', 'x', '--priority', 'asap'])).toThrow(
        /--priority must be one of/,
      )
    })

    it('rejects a dependency that is not a task id', () => {
      expect(() => parseCommand(['task', 'create', 'x', '--depends-on', 'nope'])).toThrow(
        /not a task id/,
      )
    })
  })

  describe('task list and show', () => {
    it('lists all tasks', () => {
      expect(parseCommand(['task', 'list'])).toEqual({
        name: 'task.list',
        state: undefined,
        json: false,
      })
    })

    it('filters by state', () => {
      expect(parseCommand(['task', 'list', '--state', 'review'])).toMatchObject({
        state: 'review',
      })
    })

    it('rejects an unknown state', () => {
      expect(() => parseCommand(['task', 'list', '--state', 'finished'])).toThrow(
        /--state must be one of/,
      )
    })

    it('rejects extra arguments after list', () => {
      expect(() => parseCommand(['task', 'list', 'T-1'])).toThrow(/Unexpected argument/)
    })

    it('shows one task', () => {
      expect(parseCommand(['task', 'show', 'T-42', '--json'])).toEqual({
        name: 'task.show',
        id: 'T-42',
        json: true,
      })
    })

    it('needs a valid id to show a task', () => {
      expect(() => parseCommand(['task', 'show'])).toThrow(/task id is required/)
      expect(() => parseCommand(['task', 'show', '42'])).toThrow(/not a task id/)
    })

    it('rejects an unknown task command', () => {
      expect(() => parseCommand(['task', 'remove', 'T-1'])).toThrow(/Unknown task command/)
    })
  })

  describe('event', () => {
    it('sends an event as a human with an empty payload by default', () => {
      expect(parseCommand(['event', 'T-1', 'task.readied'])).toEqual({
        name: 'event.send',
        id: 'T-1',
        type: 'task.readied',
        source: 'human',
        payload: {},
        json: false,
      })
    })

    it('reads the source and the payload', () => {
      expect(
        parseCommand([
          'event',
          'T-1',
          'state.overridden',
          '--source',
          'agent',
          '--payload',
          '{"to":"ready"}',
        ]),
      ).toMatchObject({ source: 'agent', payload: { to: 'ready' } })
    })

    it('needs an id and an event type', () => {
      expect(() => parseCommand(['event'])).toThrow(/task id is required/)
      expect(() => parseCommand(['event', 'T-1'])).toThrow(/event type is required/)
    })

    it('rejects an unknown event type and source', () => {
      expect(() => parseCommand(['event', 'T-1', 'agent.exploded'])).toThrow(
        /event type must be one of/,
      )
      expect(() => parseCommand(['event', 'T-1', 'task.readied', '--source', 'robot'])).toThrow(
        /--source must be one of/,
      )
    })

    it('rejects a payload that is not a JSON object', () => {
      expect(() => parseCommand(['event', 'T-1', 'task.readied', '--payload', '{oops'])).toThrow(
        /valid JSON/,
      )
      expect(() => parseCommand(['event', 'T-1', 'task.readied', '--payload', '[1]'])).toThrow(
        /JSON object/,
      )
      expect(() => parseCommand(['event', 'T-1', 'task.readied', '--payload', 'null'])).toThrow(
        /JSON object/,
      )
    })
  })

  describe('workspace', () => {
    it('creates a workspace', () => {
      expect(parseCommand(['workspace', 'create', 'T-1'])).toEqual({
        name: 'workspace.create',
        id: 'T-1',
        json: false,
      })
    })

    it('removes a workspace only with --yes', () => {
      expect(() => parseCommand(['workspace', 'remove', 'T-1'])).toThrow(/--yes/)
      expect(parseCommand(['workspace', 'remove', 'T-1', '--yes'])).toEqual({
        name: 'workspace.remove',
        id: 'T-1',
        deleteBranch: false,
        json: false,
      })
    })

    it('reads --delete-branch', () => {
      expect(
        parseCommand(['workspace', 'remove', 'T-1', '--yes', '--delete-branch']),
      ).toMatchObject({ deleteBranch: true })
    })

    it('needs a valid task id and a known command', () => {
      expect(() => parseCommand(['workspace', 'create'])).toThrow(/task id is required/)
      expect(() => parseCommand(['workspace', 'create', 'nope'])).toThrow(/not a task id/)
      expect(() => parseCommand(['workspace', 'list'])).toThrow(/Unknown workspace command/)
    })
  })

  it('does not let a user send an event that only the daemon records', () => {
    expect(() => parseCommand(['event', 'T-1', 'workspace.created'])).toThrow(
      /event type must be one of/,
    )
  })

  describe('events', () => {
    it('shows the event log of a task', () => {
      expect(parseCommand(['events', 'T-1'])).toEqual({
        name: 'event.list',
        id: 'T-1',
        json: false,
      })
    })
  })

  it('rejects an unknown command and an unknown option', () => {
    expect(() => parseCommand(['deploy'])).toThrow(/Unknown command "deploy"/)
    expect(() => parseCommand(['task', 'list', '--colour'])).toThrow(UsageError)
  })
})
