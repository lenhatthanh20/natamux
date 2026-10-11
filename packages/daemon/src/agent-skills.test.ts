import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
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
  const context = (agent: string) => ({ agent, workspace: repo, home })

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

  it('leaves a prompt without placeholders as it is', () => {
    expect(fillSkills('Task: {title}', context('codex'))).toEqual({
      text: 'Task: {title}',
      missing: [],
    })
  })
})
