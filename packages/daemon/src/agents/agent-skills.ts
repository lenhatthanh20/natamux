import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { isAbsolute, join, relative } from 'node:path'
import { SKILL_PLACEHOLDER, type SkillInfo, type SkillList, type SkillOrigin } from '@natamux/core'

// Where each agent finds skills, and how a prompt calls one. Checked against each agent's docs on
// 2026-10-11; see docs/agent-adapters.md. Agents change quickly: check again before relying on it.

type FoundSkill = {
  name: string
  description: string
  origin: SkillOrigin
  // The SKILL.md file.
  path: string
}

type SkillSource = {
  // Folders inside the repository, each with one folder per skill.
  project: readonly string[]
  // Folders inside the home folder.
  user: readonly string[]
  plugins?: (repo: string, home: string) => { skills: FoundSkill[]; notices: string[] }
  // How a prompt calls the skill, or null when the agent has no sure way: then it reads the file.
  call: (name: string) => string | null
}

export const SKILL_SOURCES: Record<string, SkillSource> = {
  'claude-code': {
    project: ['.claude/skills'],
    user: ['.claude/skills'],
    plugins: claudePlugins,
    call: (name) => `/${name}`,
  },
  codex: {
    project: ['.agents/skills'],
    user: ['.agents/skills'],
    call: (name) => `$${name}`,
  },
  // Gemini CLI picks a skill by its description; nothing in a prompt calls one.
  gemini: {
    project: ['.gemini/skills', '.agents/skills'],
    user: ['.gemini/skills', '.agents/skills'],
    plugins: geminiExtensions,
    call: () => null,
  },
  copilot: {
    project: ['.github/skills', '.claude/skills', '.agents/skills'],
    user: ['.copilot/skills', '.agents/skills'],
    call: (name) => `/${name}`,
  },
  // The Cursor docs do not say that the CLI calls skills by name, so it reads the file.
  cursor: {
    project: ['.agents/skills', '.cursor/skills', '.claude/skills', '.codex/skills'],
    user: ['.agents/skills', '.cursor/skills', '.claude/skills', '.codex/skills'],
    call: () => null,
  },
}

// The name and description in the front matter of a SKILL.md. Only single-line values and the
// folded `>` and `|` forms, which is what skills use.
export function readFrontMatter(text: string): { name?: string; description?: string } {
  const match = /^---\r?\n([\s\S]*?)\r?\n---/.exec(text)
  if (match?.[1] === undefined) {
    return {}
  }
  const lines = match[1].split(/\r?\n/)
  const fields: Record<string, string> = {}
  for (let index = 0; index < lines.length; index += 1) {
    const field = /^(\w[\w-]*):\s*(.*)$/.exec(lines[index] ?? '')
    if (field?.[1] === undefined) {
      continue
    }
    let value = (field[2] ?? '').trim()
    if (value === '>' || value === '|' || value === '>-' || value === '|-') {
      const block: string[] = []
      while (/^\s+\S/.test(lines[index + 1] ?? '')) {
        index += 1
        block.push((lines[index] ?? '').trim())
      }
      value = block.join(' ')
    }
    fields[field[1]] = value.replace(/^(['"])(.*)\1$/, '$2')
  }
  return { name: fields.name, description: fields.description }
}

function readSkill(folder: string, origin: SkillOrigin, prefix = ''): FoundSkill | undefined {
  const path = join(folder, 'SKILL.md')
  let text: string
  try {
    text = readFileSync(path, 'utf8').slice(0, 8000)
  } catch {
    return undefined
  }
  const front = readFrontMatter(text)
  const name = front.name ?? folder.split(/[\\/]/).at(-1) ?? ''
  return { name: `${prefix}${name}`, description: front.description ?? '', origin, path }
}

function skillsIn(folder: string, origin: SkillOrigin, prefix = ''): FoundSkill[] {
  let names: string[]
  try {
    names = readdirSync(folder, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() || entry.isSymbolicLink())
      .map((entry) => entry.name)
      .sort()
  } catch {
    return []
  }
  return names.flatMap((name) => readSkill(join(folder, name), origin, prefix) ?? [])
}

function readJson(path: string): unknown {
  try {
    return JSON.parse(readFileSync(path, 'utf8'))
  } catch {
    return undefined
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function enabledPlugins(settingsPath: string): string[] {
  const settings = readJson(settingsPath)
  const plugins = isRecord(settings) ? settings.enabledPlugins : undefined
  return isRecord(plugins)
    ? Object.entries(plugins)
        .filter(([, on]) => on === true)
        .map(([id]) => id)
    : []
}

// Claude Code plugins that are on in the repository's settings (shared) or the user's (this
// machine), found through the list of installed plugins. That list is Claude Code's own file, so
// anything unexpected in it gives no plugin skills rather than an error.
function claudePlugins(repo: string, home: string): { skills: FoundSkill[]; notices: string[] } {
  const shared = new Set([
    ...enabledPlugins(join(repo, '.claude', 'settings.json')),
    ...enabledPlugins(join(repo, '.claude', 'settings.local.json')),
  ])
  const own = enabledPlugins(join(home, '.claude', 'settings.json')).filter((id) => !shared.has(id))
  const installed = readJson(join(home, '.claude', 'plugins', 'installed_plugins.json'))
  const records = isRecord(installed) && isRecord(installed.plugins) ? installed.plugins : {}
  const skills: FoundSkill[] = []
  const notices: string[] = []
  for (const [id, origin] of [
    ...[...shared].map((id) => [id, 'plugin'] as const),
    ...own.map((id) => [id, 'user'] as const),
  ]) {
    const entries = Array.isArray(records[id]) ? (records[id] as unknown[]) : []
    const installs = entries.filter(isRecord)
    const install =
      installs.find((entry) => entry.projectPath === repo) ??
      installs.find((entry) => entry.scope === 'user') ??
      installs[0]
    const path = install?.installPath
    const plugin = id.split('@')[0] ?? id
    if (typeof path !== 'string' || !isAbsolute(path) || !existsSync(path)) {
      notices.push(`The Claude Code plugin ${plugin} is on, but not installed on this machine`)
      continue
    }
    skills.push(...skillsIn(join(path, 'skills'), origin, `${plugin}:`))
  }
  return { skills, notices }
}

// Gemini CLI extensions in the home folder may hold skills. They are this machine's.
function geminiExtensions(
  _repo: string,
  home: string,
): { skills: FoundSkill[]; notices: string[] } {
  const folder = join(home, '.gemini', 'extensions')
  let names: string[]
  try {
    names = readdirSync(folder).sort()
  } catch {
    return { skills: [], notices: [] }
  }
  const skills = names.flatMap((name) => skillsIn(join(folder, name, 'skills'), 'user'))
  return { skills, notices: [] }
}

// The skills one agent can use in `repo`, the closest first: a skill of the repository wins over
// one of the same name in the home folder.
export function agentSkills(
  agent: string,
  repo: string,
  home: string,
): { skills: FoundSkill[]; notices: string[] } {
  const source = SKILL_SOURCES[agent]
  if (source === undefined) {
    return { skills: [], notices: [] }
  }
  const plugins = source.plugins?.(repo, home) ?? { skills: [], notices: [] }
  const all = [
    ...source.project.flatMap((folder) => skillsIn(join(repo, folder), 'project')),
    ...plugins.skills,
    ...source.user.flatMap((folder) => skillsIn(join(home, folder), 'user')),
  ]
  const skills = new Map<string, FoundSkill>()
  for (const skill of all) {
    if (!skills.has(skill.name)) {
      skills.set(skill.name, skill)
    }
  }
  return { skills: [...skills.values()], notices: plugins.notices }
}

const ORIGIN_ORDER: readonly SkillOrigin[] = ['project', 'plugin', 'user']

// The skills of every agent, merged by name, for the skill picker.
export function listSkills(repo: string, home: string): SkillList {
  const merged = new Map<string, SkillInfo>()
  const notices = new Set<string>()
  for (const agent of Object.keys(SKILL_SOURCES)) {
    const found = agentSkills(agent, repo, home)
    for (const notice of found.notices) {
      notices.add(notice)
    }
    for (const skill of found.skills) {
      const known = merged.get(skill.name)
      if (known === undefined) {
        merged.set(skill.name, {
          name: skill.name,
          description: skill.description,
          origin: skill.origin,
          agents: [agent],
          path: shownPath(skill.path, repo),
        })
        continue
      }
      known.agents.push(agent)
      if (ORIGIN_ORDER.indexOf(skill.origin) < ORIGIN_ORDER.indexOf(known.origin)) {
        known.origin = skill.origin
        known.path = shownPath(skill.path, repo)
      }
      known.description ||= skill.description
    }
  }
  const skills = [...merged.values()].sort(
    (a, b) =>
      ORIGIN_ORDER.indexOf(a.origin) - ORIGIN_ORDER.indexOf(b.origin) ||
      a.name.localeCompare(b.name),
  )
  return { skills, notices: [...notices] }
}

// A path inside the repository is shown from its root; one elsewhere in full.
function shownPath(path: string, root: string): string {
  const inside = relative(root, path)
  return inside === '' || inside.startsWith('..') || isAbsolute(inside)
    ? path
    : inside.split('\\').join('/')
}

export type SkillContext = {
  agent: string
  // The task's worktree: its skills are the ones of the branch the agent works on.
  workspace: string
  // The main checkout. Its untracked files, such as `.claude/settings.local.json`, are not in a
  // new worktree.
  repo: string
  home: string
}

export type SkillResolution =
  | { kind: 'call'; text: string }
  | { kind: 'file'; path: string }
  | { kind: 'missing' }

// How the task's agent gets a skill: its own call when it has the skill and a way to call it,
// else the SKILL.md to read (of its own, or of another agent, in the worktree and then in the
// main checkout), else nothing. A skill of the main checkout only is never called: the agent in
// the worktree may not have it.
export function resolveSkill(name: string, context: SkillContext): SkillResolution {
  const own = agentSkills(context.agent, context.workspace, context.home).skills.find(
    (skill) => skill.name === name,
  )
  const call = own === undefined ? null : (SKILL_SOURCES[context.agent]?.call(name) ?? null)
  if (call !== null) {
    return { kind: 'call', text: call }
  }
  const folders =
    context.repo === context.workspace ? [context.workspace] : [context.workspace, context.repo]
  const file =
    own ??
    folders
      .flatMap((folder) =>
        Object.keys(SKILL_SOURCES).flatMap(
          (agent) => agentSkills(agent, folder, context.home).skills,
        ),
      )
      .find((skill) => skill.name === name)
  return file === undefined
    ? { kind: 'missing' }
    : { kind: 'file', path: shownPath(file.path, context.workspace) }
}

// Fills each `{skill:<name>}` of a prompt for the task's agent. The text around a placeholder
// stays as it is, so each form reads well inside a sentence.
export function fillSkills(
  text: string,
  context: SkillContext,
): { text: string; missing: string[] } {
  const missing: string[] = []
  const filled = text.replace(SKILL_PLACEHOLDER, (_whole, name: string) => {
    const found = resolveSkill(name, context)
    switch (found.kind) {
      case 'call':
        return found.text
      case 'file':
        return `the skill in ${found.path} (read that file and follow it)`
      case 'missing':
        if (!missing.includes(name)) {
          missing.push(name)
        }
        return `the ${name} skill`
    }
  })
  return { text: filled, missing }
}
