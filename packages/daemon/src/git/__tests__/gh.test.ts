import { describe, expect, it } from 'vitest'
import {
  createPullRequest,
  findPullRequest,
  findPullRequests,
  GitHubError,
  ghError,
  parsePullRequestBatch,
  parsePullRequestRef,
  parsePullRequestView,
  pullRequestUrl,
  type RunGh,
  readPullRequestStatus,
  readPullRequestStatuses,
  summarizeChecks,
} from '../gh.js'

describe('pull requests', () => {
  const request = { branch: 'natamux/t-0001', title: 'Fix it', body: 'Body' }

  it.each([
    ['https://github.com/o/r/pull/12\n', 'https://github.com/o/r/pull/12'],
    [
      'Creating pull request\nhttps://ghe.example.com/o/r/pull/3',
      'https://ghe.example.com/o/r/pull/3',
    ],
    ['nothing here', undefined],
  ])('finds the address in %j', (output, url) => {
    expect(pullRequestUrl(output)).toBe(url)
  })

  it('asks gh for a pull request from the branch and gives its address', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return 'https://github.com/o/r/pull/12\n'
    }

    expect(await createPullRequest('/w', request, gh)).toBe('https://github.com/o/r/pull/12')
    expect(calls).toEqual([
      ['pr', 'create', '--head', 'natamux/t-0001', '--title', 'Fix it', '--body', 'Body'],
    ])
  })

  it('gives the address of the pull request that exists already', async () => {
    const gh: RunGh = async () => {
      throw new GitHubError(
        'a pull request for branch "natamux/t-0001" into branch "main" already exists:\n' +
          'https://github.com/o/r/pull/9',
      )
    }
    expect(await createPullRequest('/w', request, gh)).toBe('https://github.com/o/r/pull/9')
  })

  it('passes on any other failure', async () => {
    const gh: RunGh = async () => {
      throw new GitHubError('not logged in')
    }
    await expect(createPullRequest('/w', request, gh)).rejects.toThrow('not logged in')
  })

  it('fails when gh answers without an address', async () => {
    const gh: RunGh = async () => 'ok'
    await expect(createPullRequest('/w', request, gh)).rejects.toThrow(GitHubError)
  })
})

describe('summarizeChecks', () => {
  it.each([
    [[], 'none'],
    [[{ status: 'COMPLETED', conclusion: 'SUCCESS' }, { state: 'SUCCESS' }], 'passing'],
    [[{ status: 'COMPLETED', conclusion: 'SKIPPED' }], 'passing'],
    [[{ status: 'IN_PROGRESS' }, { status: 'COMPLETED', conclusion: 'SUCCESS' }], 'pending'],
    [[{ state: 'PENDING' }], 'pending'],
    [[{ status: 'IN_PROGRESS' }, { status: 'COMPLETED', conclusion: 'FAILURE' }], 'failing'],
    [[{ state: 'ERROR' }], 'failing'],
    [[{ status: 'COMPLETED', conclusion: 'TIMED_OUT' }], 'failing'],
  ] as const)('reads %j as %s', (items, summary) => {
    expect(summarizeChecks(items)).toBe(summary)
  })
})

describe('parsePullRequestView', () => {
  it.each([
    [
      { state: 'OPEN', isDraft: false, statusCheckRollup: [] },
      { state: 'open', checks: 'none' },
    ],
    [
      { state: 'OPEN', isDraft: true, statusCheckRollup: null },
      { state: 'draft', checks: 'none' },
    ],
    [
      { state: 'MERGED', isDraft: false, statusCheckRollup: [{ state: 'SUCCESS' }] },
      { state: 'merged', checks: 'passing' },
    ],
    [{ state: 'CLOSED' }, { state: 'closed', checks: 'none' }],
  ])('reads %j', (data, status) => {
    expect(parsePullRequestView(JSON.stringify(data))).toEqual(status)
  })
})

describe('readPullRequestStatus', () => {
  it('asks gh for the state and the checks of the pull request', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify({ state: 'OPEN', isDraft: false, statusCheckRollup: [] })
    }

    expect(await readPullRequestStatus('/repo', 'https://github.com/o/r/pull/1', gh)).toEqual({
      state: 'open',
      checks: 'none',
    })
    expect(calls).toEqual([
      ['pr', 'view', 'https://github.com/o/r/pull/1', '--json', 'state,isDraft,statusCheckRollup'],
    ])
  })

  it('fails with a GitHubError on an answer that is not JSON', async () => {
    const gh: RunGh = async () => 'not json'
    const reading = readPullRequestStatus('/repo', 'https://github.com/o/r/pull/1', gh)
    await expect(reading).rejects.toThrow(GitHubError)
  })
})

describe('findPullRequest', () => {
  const answer =
    (list: object[]): RunGh =>
    async () =>
      JSON.stringify(list)

  it('asks gh for the newest pull request from the branch', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify([{ url: 'https://github.com/o/r/pull/7', state: 'OPEN' }])
    }

    expect(await findPullRequest('/w', 'natamux/t-0001', gh)).toBe('https://github.com/o/r/pull/7')
    expect(calls.map((args) => args.join(' '))).toEqual([
      'pr list --head natamux/t-0001 --state all --json url,state --limit 1',
    ])
  })

  it('finds a merged pull request', async () => {
    const gh = answer([{ url: 'https://github.com/o/r/pull/8', state: 'MERGED' }])
    expect(await findPullRequest('/w', 'b', gh)).toBe('https://github.com/o/r/pull/8')
  })

  it.each([
    ['no pull request', []],
    ['a closed one', [{ url: 'https://github.com/o/r/pull/9', state: 'CLOSED' }]],
  ])('finds nothing for %s', async (_name, list) => {
    expect(await findPullRequest('/w', 'b', answer(list))).toBeUndefined()
  })

  it('fails with a GitHubError on an answer that is not JSON', async () => {
    const gh: RunGh = async () => 'not json'
    await expect(findPullRequest('/w', 'b', gh)).rejects.toThrow(GitHubError)
  })
})

describe('findPullRequests', () => {
  it('asks gh once for the newest pull requests of the repository, by branch', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify([
        { url: 'https://github.com/o/r/pull/9', state: 'OPEN', headRefName: 'a' },
        { url: 'https://github.com/o/r/pull/8', state: 'MERGED', headRefName: 'b' },
        { url: 'https://github.com/o/r/pull/7', state: 'MERGED', headRefName: 'a' },
      ])
    }

    expect(await findPullRequests('/w', gh)).toEqual(
      new Map([
        ['a', 'https://github.com/o/r/pull/9'],
        ['b', 'https://github.com/o/r/pull/8'],
      ]),
    )
    expect(calls.map((args) => args.join(' '))).toEqual([
      'pr list --state all --json url,state,headRefName --limit 100',
    ])
  })

  it('leaves out a branch whose newest pull request is closed', async () => {
    const gh: RunGh = async () =>
      JSON.stringify([
        { url: 'https://github.com/o/r/pull/9', state: 'CLOSED', headRefName: 'a' },
        { url: 'https://github.com/o/r/pull/7', state: 'MERGED', headRefName: 'a' },
      ])
    expect(await findPullRequests('/w', gh)).toEqual(new Map())
  })

  it('fails with a GitHubError on an answer that is not a JSON list', async () => {
    await expect(findPullRequests('/w', async () => 'not json')).rejects.toThrow(GitHubError)
    await expect(findPullRequests('/w', async () => '{}')).rejects.toThrow(GitHubError)
  })
})

// A pull request as `gh api graphql` gives it, with the checks of its last commit.
function pullRequestNode(state: string, isDraft: boolean, checks: object[] | null) {
  const rollup = checks === null ? null : { contexts: { nodes: checks } }
  return { state, isDraft, commits: { nodes: [{ commit: { statusCheckRollup: rollup } }] } }
}

describe('parsePullRequestRef', () => {
  it.each([
    ['https://github.com/o/r/pull/12', { host: 'github.com', owner: 'o', name: 'r', number: 12 }],
    [
      'https://ghe.example.com/team/app/pull/3',
      { host: 'ghe.example.com', owner: 'team', name: 'app', number: 3 },
    ],
    ['https://github.com/o/r/issues/12', undefined],
    ['not an address', undefined],
  ])('reads %j', (url, ref) => {
    expect(parsePullRequestRef(url)).toEqual(ref)
  })
})

describe('parsePullRequestBatch', () => {
  it('maps each pull request as parsePullRequestView does', () => {
    const cases = [
      [
        pullRequestNode('OPEN', false, []),
        { state: 'OPEN', isDraft: false, statusCheckRollup: [] },
      ],
      [pullRequestNode('OPEN', true, null), { state: 'OPEN', isDraft: true }],
      [
        pullRequestNode('MERGED', false, [{ state: 'SUCCESS' }]),
        { state: 'MERGED', isDraft: false, statusCheckRollup: [{ state: 'SUCCESS' }] },
      ],
      [
        pullRequestNode('OPEN', false, [{ status: 'IN_PROGRESS', conclusion: null }]),
        { state: 'OPEN', isDraft: false, statusCheckRollup: [{ status: 'IN_PROGRESS' }] },
      ],
      [
        pullRequestNode('OPEN', false, [{ status: 'COMPLETED', conclusion: 'FAILURE' }]),
        {
          state: 'OPEN',
          isDraft: false,
          statusCheckRollup: [{ status: 'COMPLETED', conclusion: 'FAILURE' }],
        },
      ],
      [{ state: 'CLOSED', isDraft: false, commits: { nodes: [] } }, { state: 'CLOSED' }],
    ] as const
    const repository = Object.fromEntries(cases.map(([node], at) => [`pr${at + 1}`, node]))
    const statuses = parsePullRequestBatch(
      JSON.stringify({ data: { repository } }),
      cases.map((_, at) => at + 1),
    )

    for (const [at, [, view]] of cases.entries()) {
      expect(statuses.get(at + 1)).toEqual(parsePullRequestView(JSON.stringify(view)))
    }
  })

  it('leaves out a number that GitHub did not find', () => {
    const repository = { pr1: pullRequestNode('OPEN', false, []), pr2: null }
    const statuses = parsePullRequestBatch(JSON.stringify({ data: { repository } }), [1, 2])
    expect([...statuses.keys()]).toEqual([1])
  })

  it('fails without a repository in the answer', () => {
    const output = JSON.stringify({ data: { repository: null } })
    expect(() => parsePullRequestBatch(output, [1])).toThrow()
  })
})

describe('readPullRequestStatuses', () => {
  const repo = { host: 'github.com', owner: 'o', name: 'r' }

  it('asks for all pull requests of a repository in one GraphQL call', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      const repository = { pr1: pullRequestNode('OPEN', false, []), pr7: null }
      return JSON.stringify({ data: { repository } })
    }

    const statuses = await readPullRequestStatuses('/repo', repo, [1, 7], gh)

    expect(statuses).toEqual(new Map([[1, { state: 'open', checks: 'none' }]]))
    expect(calls).toHaveLength(1)
    const [args = []] = calls
    expect(args.slice(0, 2)).toEqual(['api', 'graphql'])
    expect(args).toContain('owner=o')
    expect(args).toContain('name=r')
    expect(args).not.toContain('--hostname')
    const query = args.find((arg) => arg.startsWith('query=')) ?? ''
    expect(query).toContain('pr1: pullRequest(number: 1)')
    expect(query).toContain('pr7: pullRequest(number: 7)')
  })

  it('names the host of GitHub Enterprise', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify({ data: { repository: { pr1: pullRequestNode('OPEN', false, []) } } })
    }

    await readPullRequestStatuses('/repo', { ...repo, host: 'ghe.example.com' }, [1], gh)

    expect(calls[0]?.slice(2, 4)).toEqual(['--hostname', 'ghe.example.com'])
  })

  it('reads the answer gh printed before it failed for a number it did not find', async () => {
    const gh: RunGh = async () => {
      const repository = { pr1: pullRequestNode('MERGED', false, []), pr2: null }
      throw new GitHubError(
        'Could not resolve to a PullRequest',
        JSON.stringify({ data: { repository } }),
      )
    }

    expect(await readPullRequestStatuses('/repo', repo, [1, 2], gh)).toEqual(
      new Map([[1, { state: 'merged', checks: 'none' }]]),
    )
  })

  it('passes on a failure without an answer', async () => {
    const gh: RunGh = async () => {
      throw new GitHubError('not logged in')
    }
    await expect(readPullRequestStatuses('/repo', repo, [1], gh)).rejects.toThrow('not logged in')
  })

  it('fails with a GitHubError on an answer that is not JSON', async () => {
    const gh: RunGh = async () => 'not json'
    await expect(readPullRequestStatuses('/repo', repo, [1], gh)).rejects.toThrow(GitHubError)
  })
})

describe('ghError', () => {
  it('says gh timed out when the time limit killed it', () => {
    const killed = Object.assign(new Error('Command failed: gh pr view'), {
      killed: true,
      signal: 'SIGTERM',
      stdout: '',
    })
    expect(ghError(killed).message).toBe('GitHub CLI failed: gh did not finish in 60 seconds')
  })

  it('keeps the stderr and stdout of a gh that failed by itself', () => {
    const failed = Object.assign(new Error('Command failed'), {
      killed: false,
      stderr: 'not logged in\n',
      stdout: '{"data":{}}',
    })
    const error = ghError(failed)
    expect(error.message).toBe('GitHub CLI failed: not logged in')
    expect(error.output).toBe('{"data":{}}')
  })
})
