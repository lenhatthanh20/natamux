// Skills that agents can use, as the daemon lists them for the skill picker of the settings page.

// Where a skill comes from: the repository (shared), a plugin the repository turns on (shared),
// or the home folder of this machine (not shared).
export type SkillOrigin = 'project' | 'plugin' | 'user'

export type SkillInfo = {
  // The name a prompt uses in `{skill:<name>}`. A plugin skill is `<plugin>:<skill>`.
  name: string
  description: string
  origin: SkillOrigin
  // The agents that have the skill.
  agents: string[]
  // The SKILL.md file: from the repository root when it is inside the repository.
  path: string
}

export type SkillList = {
  skills: SkillInfo[]
  // Why some skills may be missing, such as a plugin that is on but not installed.
  notices: string[]
}
