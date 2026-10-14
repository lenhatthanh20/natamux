import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { LAUNCHABLE_AGENTS } from './agent-launch.js'
import {
  agentSkills,
  fillSkills,
  listSkills,
  readFrontMatter,
  resolveSkill,
  SKILL_SOURCES,
} from './agent-skills.js'

let root: string
let repo: string
let home: string

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'natamux-skills-'))
  repo = join(root, 'repo')
  home = join(root, 'home')
  mkdirSync(repo)
  mkdirSync(home)
})

afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

function skill(folder: string, name: string, description = `${name} help`): string {
  const dir = join(folder, name)
  mkdirSync(dir, { recursive: true })
  const path = join(dir, 'SKILL.md')
  writeFileSync(path, `---\nname: ${name}\ndescription: ${description}\n---\n\nSteps.\n`)
  return path
}

function json(path: string, value: unknown): void {
  mkdirSync(join(path, '..'), { recursive: true })
  writeFileSync(path, JSON.stringify(value))
}

describe('readFrontMatter', () => {
  it('reads one-line and folded values, without quotes', () => {
    const text = [
      '---',
      'name: "fetch-ticket"',
      'description: >',
      '  Reads a ticket',
      '  with its comments.',
      'license: MIT',
      '---',
      'Body',
    ].join('\n')
    expect(readFrontMatter(text)).toEqual({
      name: 'fetch-ticket',
      description: 'Reads a ticket with its comments.',
    })
  })

  it('gives nothing for a file without front matter', () => {
    expect(readFrontMatter('# Just text')).toEqual({})
  })
})

describe('agentSkills', () => {
  it('has a skill source for every agent Natamux starts', () => {
    expect(Object.keys(SKILL_SOURCES).sort()).toEqual([...LAUNCHABLE_AGENTS].sort())
  })

  it("reads each agent's own folders", () => {
    skill(join(repo, '.claude', 'skills'), 'claude-only')
    skill(join(repo, '.agents', 'skills'), 'shared')
    skill(join(repo, '.github', 'skills'), 'copilot-only')
    skill(join(home, '.gemini', 'skills'), 'gemini-mine')
    const names = (agent: string) =>
      agentSkills(agent, repo, home).skills.map((found) => found.name)

    expect(names('claude-code')).toEqual(['claude-only'])
    expect(names('codex')).toEqual(['shared'])
    expect(names('gemini')).toEqual(['shared', 'gemini-mine'])
    expect(names('copilot')).toEqual(['copilot-only', 'claude-only', 'shared'])
    expect(names('cursor')).toEqual(['shared', 'claude-only'])
    expect(names('aider')).toEqual([])
  })

  it('prefers a skill of the repository to one of the same name in the home folder', () => {
    skill(join(repo, '.claude', 'skills'), 'review', 'Project review')
    skill(join(home, '.claude', 'skills'), 'review', 'My review')
    const [found] = agentSkills('claude-code', repo, home).skills
    expect(found).toMatchObject({ origin: 'project', description: 'Project review' })
  })

  it('reads the skills of Claude Code plugins that are on and installed', () => {
    const shared = join(home, '.claude', 'plugins', 'cache', 'm', 'design', '1.0')
    const mine = join(home, '.claude', 'plugins', 'cache', 'm', 'notes', '2.0')
    skill(join(shared, 'skills'), 'frontend')
    skill(join(mine, 'skills'), 'jot')
    json(join(repo, '.claude', 'settings.json'), {
      enabledPlugins: { 'design@m': true, 'off@m': false, 'gone@m': true },
    })
    json(join(home, '.claude', 'settings.json'), { enabledPlugins: { 'notes@m': true } })
    json(join(home, '.claude', 'plugins', 'installed_plugins.json'), {
      version: 2,
      plugins: {
        'design@m': [{ scope: 'project', projectPath: repo, installPath: shared }],
        'notes@m': [{ scope: 'user', installPath: mine }],
      },
    })

    const found = agentSkills('claude-code', repo, home)
    expect(found.skills.map(({ name, origin }) => ({ name, origin }))).toEqual([
      { name: 'design:frontend', origin: 'plugin' },
      { name: 'notes:jot', origin: 'user' },
    ])
    expect(found.notices).toEqual([
      'The Claude Code plugin gone is on, but not installed on this machine',
    ])
  })

  it('gives no plugin skills, and no error, when the plugin files are not what it expects', () => {
    json(join(repo, '.claude', 'settings.json'), { enabledPlugins: ['design@m'] })
    writeFileSync(join(repo, '.claude', 'settings.local.json'), '{ broken')
    json(join(home, '.claude', 'plugins', 'installed_plugins.json'), { plugins: 'nope' })
    expect(agentSkills('claude-code', repo, home)).toEqual({ skills: [], notices: [] })
  })

  it('names a skill without front matter by its folder, and skips a folder without SKILL.md', () => {
    mkdirSync(join(repo, '.claude', 'skills', 'plain'), { recursive: true })
    writeFileSync(join(repo, '.claude', 'skills', 'plain', 'SKILL.md'), '# Plain\n')
    mkdirSync(join(repo, '.claude', 'skills', 'empty'), { recursive: true })
    expect(agentSkills('claude-code', repo, home).skills).toEqual([
      {
        name: 'plain',
        description: '',
        origin: 'project',
        path: join(repo, '.claude', 'skills', 'plain', 'SKILL.md'),
      },
    ])
  })

  it.skipIf(process.platform === 'win32')('skips a skill folder that is a broken link', () => {
    mkdirSync(join(repo, '.claude', 'skills'), { recursive: true })
    symlinkSync(join(root, 'nowhere'), join(repo, '.claude', 'skills', 'gone'))
    skill(join(repo, '.claude', 'skills'), 'kept')
    expect(agentSkills('claude-code', repo, home).skills.map((found) => found.name)).toEqual([
      'kept',
    ])
  })

  it('gives no plugin skills when the list of installed plugins is not JSON', () => {
    json(join(repo, '.claude', 'settings.json'), { enabledPlugins: { 'design@m': true } })
    mkdirSync(join(home, '.claude', 'plugins'), { recursive: true })
    writeFileSync(join(home, '.claude', 'plugins', 'installed_plugins.json'), 'nope')
    skill(join(repo, '.claude', 'skills'), 'review')
    const found = agentSkills('claude-code', repo, home)
    expect(found.skills.map((each) => each.name)).toEqual(['review'])
    expect(found.notices).toEqual([
      'The Claude Code plugin design is on, but not installed on this machine',
    ])
  })

  it('reads the skills of Gemini CLI extensions', () => {
    skill(join(home, '.gemini', 'extensions', 'tools', 'skills'), 'lint-fix')
    expect(agentSkills('gemini', repo, home).skills.map((found) => found.name)).toEqual([
      'lint-fix',
    ])
  })
})

describe('listSkills', () => {
  it('merges the skills of all agents by name, the shared ones first', () => {
    skill(join(home, '.agents', 'skills'), 'mine')
    skill(join(repo, '.claude', 'skills'), 'review')
    skill(join(repo, '.agents', 'skills'), 'review')
    const list = listSkills(repo, home)
    expect(list.skills).toEqual([
      {
        name: 'review',
        description: 'review help',
        origin: 'project',
        agents: ['claude-code', 'codex', 'gemini', 'copilot', 'cursor'],
        path: '.claude/skills/review/SKILL.md',
      },
      {
        name: 'mine',
        description: 'mine help',
        origin: 'user',
        agents: ['codex', 'gemini', 'copilot', 'cursor'],
        path: join(home, '.agents', 'skills', 'mine', 'SKILL.md'),
      },
    ])
    expect(list.notices).toEqual([])
  })
})

describe('resolveSkill and fillSkills', () => {
  const context = (agent: string) => ({ agent, workspace: repo, repo, home })

  it("uses the agent's own call when it has the skill", () => {
    skill(join(repo, '.claude', 'skills'), 'review')
    skill(join(repo, '.agents', 'skills'), 'review')
    skill(join(repo, '.github', 'skills'), 'review')
    expect(resolveSkill('review', context('claude-code'))).toEqual({
      kind: 'call',
      text: '/review',
    })
    expect(resolveSkill('review', context('codex'))).toEqual({ kind: 'call', text: '$review' })
    expect(resolveSkill('review', context('copilot'))).toEqual({ kind: 'call', text: '/review' })
  })

  it('gives the file to read to an agent that has no call, or not the skill', () => {
    skill(join(repo, '.claude', 'skills'), 'review')
    const file = { kind: 'file', path: '.claude/skills/review/SKILL.md' }
    expect(resolveSkill('review', context('gemini'))).toEqual(file)
    expect(resolveSkill('review', context('cursor'))).toEqual(file)
    expect(resolveSkill('review', context('codex'))).toEqual(file)
    expect(resolveSkill('review', context('aider'))).toEqual(file)
  })

  it('calls a plugin skill by its plugin name', () => {
    const install = join(home, '.claude', 'plugins', 'cache', 'm', 'acme', '1.0')
    skill(join(install, 'skills'), 'lint-fix')
    json(join(repo, '.claude', 'settings.json'), { enabledPlugins: { 'acme@m': true } })
    json(join(home, '.claude', 'plugins', 'installed_plugins.json'), {
      plugins: { 'acme@m': [{ scope: 'user', installPath: install }] },
    })
    expect(resolveSkill('acme:lint-fix', context('claude-code'))).toEqual({
      kind: 'call',
      text: '/acme:lint-fix',
    })
  })

  it('prefers the skill of the worktree to one of the same name in the home folder', () => {
    skill(join(repo, '.claude', 'skills'), 'review')
    skill(join(home, '.claude', 'skills'), 'review')
    expect(resolveSkill('review', context('gemini'))).toEqual({
      kind: 'file',
      path: '.claude/skills/review/SKILL.md',
    })
  })

  it('prints a path with spaces whole', () => {
    const spaced = join(root, 'my home')
    const path = skill(join(spaced, '.claude', 'skills'), 'review')
    const other = { agent: 'codex', workspace: repo, repo, home: spaced }
    expect(fillSkills('Use {skill:review}.', other)).toEqual({
      text: `Use the skill in ${path} (read that file and follow it).`,
      missing: [],
    })
  })

  it('leaves a placeholder with no name or other characters as it is', () => {
    expect(fillSkills('{skill:} {skill:a b} {skill:ok?}', context('codex'))).toEqual({
      text: '{skill:} {skill:a b} {skill:ok?}',
      missing: [],
    })
  })

  it('says when no agent has the skill', () => {
    expect(resolveSkill('gone', context('claude-code'))).toEqual({ kind: 'missing' })
  })

  it('fills each placeholder of a prompt and lists the missing skills once', () => {
    skill(join(repo, '.agents', 'skills'), 'review')
    skill(join(repo, '.claude', 'skills'), 'design')
    const filled = fillSkills(
      'Use {skill:review}, then {skill:design}, then {skill:gone} and {skill:gone}.',
      context('codex'),
    )
    expect(filled).toEqual({
      text: 'Use $review, then the skill in .claude/skills/design/SKILL.md (read that file and follow it), then the gone skill and the gone skill.',
      missing: ['gone'],
    })
  })

  describe('in a worktree of the repository', () => {
    let worktree: string
    const inWorktree = (agent: string) => ({ agent, workspace: worktree, repo, home })

    beforeEach(() => {
      worktree = join(root, 'worktree')
      mkdirSync(worktree)
    })

    it('reads a plugin skill that only the untracked settings of the main checkout turn on', () => {
      const install = join(home, '.claude', 'plugins', 'cache', 'm', 'acme', '1.0')
      const path = skill(join(install, 'skills'), 'lint-fix')
      json(join(repo, '.claude', 'settings.local.json'), { enabledPlugins: { 'acme@m': true } })
      json(join(home, '.claude', 'plugins', 'installed_plugins.json'), {
        plugins: { 'acme@m': [{ scope: 'local', projectPath: repo, installPath: install }] },
      })
      expect(fillSkills('Use {skill:acme:lint-fix}.', inWorktree('claude-code'))).toEqual({
        text: `Use the skill in ${path} (read that file and follow it).`,
        missing: [],
      })
    })

    it('reads a skill of the main checkout that the worktree does not have', () => {
      const path = skill(join(repo, '.claude', 'skills'), 'review')
      expect(resolveSkill('review', inWorktree('claude-code'))).toEqual({ kind: 'file', path })
    })

    it('prefers the skill of the worktree to the one of the main checkout', () => {
      skill(join(repo, '.claude', 'skills'), 'review')
      skill(join(worktree, '.claude', 'skills'), 'review')
      expect(resolveSkill('review', inWorktree('claude-code'))).toEqual({
        kind: 'call',
        text: '/review',
      })
      expect(resolveSkill('review', inWorktree('gemini'))).toEqual({
        kind: 'file',
        path: '.claude/skills/review/SKILL.md',
      })
    })

    it('says when neither has the skill', () => {
      expect(resolveSkill('gone', inWorktree('claude-code'))).toEqual({ kind: 'missing' })
    })

    // A plugin `acme@m` with the skill `lint-fix`, installed for the main checkout.
    function plugin(skills = ['lint-fix']): string {
      const install = join(home, '.claude', 'plugins', 'cache', 'm', 'acme', '1.0')
      mkdirSync(join(install, 'skills'), { recursive: true })
      for (const name of skills) {
        skill(join(install, 'skills'), name)
      }
      json(join(home, '.claude', 'plugins', 'installed_plugins.json'), {
        plugins: { 'acme@m': [{ scope: 'local', projectPath: repo, installPath: install }] },
      })
      return install
    }

    it('reads a plugin that the committed settings of the main checkout turn on', () => {
      const install = plugin()
      json(join(repo, '.claude', 'settings.json'), { enabledPlugins: { 'acme@m': true } })
      expect(resolveSkill('acme:lint-fix', inWorktree('claude-code'))).toEqual({
        kind: 'file',
        path: join(install, 'skills', 'lint-fix', 'SKILL.md'),
      })
    })

    it('finds no plugin skill that is off, not installed, or without SKILL.md', () => {
      json(join(repo, '.claude', 'settings.local.json'), { enabledPlugins: { 'acme@m': false } })
      plugin()
      expect(resolveSkill('acme:lint-fix', inWorktree('claude-code'))).toEqual({ kind: 'missing' })

      json(join(repo, '.claude', 'settings.local.json'), { enabledPlugins: { 'acme@m': true } })
      expect(resolveSkill('acme:other', inWorktree('claude-code'))).toEqual({ kind: 'missing' })

      json(join(home, '.claude', 'plugins', 'installed_plugins.json'), { plugins: {} })
      expect(resolveSkill('acme:lint-fix', inWorktree('claude-code'))).toEqual({ kind: 'missing' })
    })

    it('resolves the plugin skills and the plain ones of one prompt', () => {
      const install = plugin(['worktree'])
      json(join(repo, '.claude', 'settings.local.json'), { enabledPlugins: { 'acme@m': true } })
      const own = skill(join(repo, '.claude', 'skills'), 'review')
      const filled = fillSkills(
        'Use {skill:acme:worktree}, {skill:review} and {skill:gone}.',
        inWorktree('claude-code'),
      )
      expect(filled).toEqual({
        text: `Use the skill in ${join(install, 'skills', 'worktree', 'SKILL.md')} (read that file and follow it), the skill in ${own} (read that file and follow it) and the gone skill.`,
        missing: ['gone'],
      })
    })

    it("reads another agent's skill of the main checkout, not a folder without SKILL.md", () => {
      const path = skill(join(repo, '.agents', 'skills'), 'review')
      mkdirSync(join(repo, '.claude', 'skills', 'empty'), { recursive: true })
      expect(resolveSkill('review', inWorktree('claude-code'))).toEqual({ kind: 'file', path })
      expect(resolveSkill('empty', inWorktree('claude-code'))).toEqual({ kind: 'missing' })
    })

    it('shows a main checkout path whole, also with spaces or around the worktree', () => {
      const spaced = join(root, 'Application Support', 'repo')
      const path = skill(join(spaced, '.claude', 'skills'), 'review')
      const nested = join(spaced, '.worktrees', 'T-1')
      mkdirSync(nested, { recursive: true })
      for (const workspace of [worktree, nested]) {
        const context = { agent: 'claude-code', workspace, repo: spaced, home }
        expect(fillSkills('Use {skill:review}.', context)).toEqual({
          text: `Use the skill in ${path} (read that file and follow it).`,
          missing: [],
        })
      }
    })

    it('keeps the call and the relative paths of the worktree', () => {
      plugin()
      json(join(worktree, '.claude', 'settings.json'), { enabledPlugins: { 'acme@m': true } })
      json(join(repo, '.claude', 'settings.local.json'), { enabledPlugins: { 'acme@m': true } })
      skill(join(worktree, '.agents', 'skills'), 'design')
      skill(join(repo, '.claude', 'skills'), 'design')
      expect(resolveSkill('acme:lint-fix', inWorktree('claude-code'))).toEqual({
        kind: 'call',
        text: '/acme:lint-fix',
      })
      expect(resolveSkill('design', inWorktree('claude-code'))).toEqual({
        kind: 'file',
        path: '.agents/skills/design/SKILL.md',
      })
    })

    it('gives no error for a main checkout that is gone or has broken settings', () => {
      plugin()
      mkdirSync(join(repo, '.claude'), { recursive: true })
      writeFileSync(join(repo, '.claude', 'settings.local.json'), '{ not json')
      expect(resolveSkill('acme:lint-fix', inWorktree('claude-code'))).toEqual({ kind: 'missing' })
      const gone = { agent: 'claude-code', workspace: worktree, repo: join(root, 'gone'), home }
      expect(fillSkills('{skill:review}', gone)).toEqual({
        text: 'the review skill',
        missing: ['review'],
      })
    })

    it('resolves in the worktree every skill the picker of the main checkout offers', () => {
      plugin(['lint-fix', 'worktree'])
      json(join(repo, '.claude', 'settings.local.json'), { enabledPlugins: { 'acme@m': true } })
      skill(join(repo, '.claude', 'skills'), 'review')
      skill(join(repo, '.agents', 'skills'), 'design')
      const offered = listSkills(repo, home).skills.map((found) => found.name)
      expect(offered).toEqual(expect.arrayContaining(['acme:lint-fix', 'acme:worktree', 'review']))
      for (const name of offered) {
        expect(resolveSkill(name, inWorktree('claude-code')).kind).toBe('file')
      }
    })

    it('treats the main checkout given with a trailing slash as the worktree', () => {
      skill(join(repo, '.claude', 'skills'), 'review')
      const same = { agent: 'claude-code', workspace: repo, repo: `${repo}/`, home }
      expect(resolveSkill('review', same)).toEqual({ kind: 'call', text: '/review' })
    })
  })

  it('leaves a prompt without placeholders as it is', () => {
    expect(fillSkills('Task: {title}', context('codex'))).toEqual({
      text: 'Task: {title}',
      missing: [],
    })
  })
})
