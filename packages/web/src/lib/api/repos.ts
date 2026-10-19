import {
  type GitHubIssue,
  githubIssueSchema,
  projectSettingsSchema,
  type Repo,
  repoSchema,
  type SkillList,
  type Task,
  taskSchema,
} from '@natamux/core'
import type { Client } from './client'
import { asRecord, skillInfo, stringList } from './parse'
import type { LoadedSettings, SavedSettings } from './types'

export function reposApi(client: Client) {
  const { request } = client

  return {
    // `worktreesDir`: the daemon's worktree folder. An older daemon does not send it.
    async listRepos(): Promise<{ repos: Repo[]; worktreesDir?: string }> {
      const body = asRecord(await request('GET', '/repos'))
      if (!Array.isArray(body.repos)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      const repos = body.repos.map((repo) => repoSchema.parse(repo))
      return typeof body.worktreesDir === 'string'
        ? { repos, worktreesDir: body.worktreesDir }
        : { repos }
    },

    async addRepo(path: string, name?: string, worktreesDir?: string): Promise<Repo> {
      const body = asRecord(await request('POST', '/repos', { path, name, worktreesDir }))
      return repoSchema.parse(body.repo)
    },

    async removeRepo(id: string): Promise<void> {
      await request('DELETE', `/repos/${encodeURIComponent(id)}`)
    },

    async listIssues(repo?: string): Promise<GitHubIssue[]> {
      const query = repo === undefined ? '' : `?repo=${encodeURIComponent(repo)}`
      const body = asRecord(await request('GET', `/github/issues${query}`))
      if (!Array.isArray(body.issues)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.issues.map((issue) => githubIssueSchema.parse(issue))
    },

    // Makes a draft task from each issue. Issues imported before are skipped.
    async importIssues(numbers: readonly number[], repo?: string): Promise<Task[]> {
      const body = asRecord(await request('POST', '/github/issues/import', { numbers, repo }))
      if (!Array.isArray(body.tasks)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.tasks.map((task) => taskSchema.parse(task))
    },

    async getSettings(repo: string): Promise<LoadedSettings> {
      const body = asRecord(await request('GET', `/repos/${encodeURIComponent(repo)}/settings`))
      return {
        settings: projectSettingsSchema.parse(body.settings),
        error: typeof body.error === 'string' ? body.error : null,
        found: body.found === true,
        path: String(body.path ?? ''),
        text: typeof body.text === 'string' ? body.text : null,
        version: typeof body.version === 'string' ? body.version : null,
      }
    },

    // With `version`, the daemon answers 409 (see isConflict) when the file changed since then.
    async saveSettings(repo: string, raw: unknown, version?: string): Promise<SavedSettings> {
      const body = asRecord(
        await request(
          'PUT',
          `/repos/${encodeURIComponent(repo)}/settings`,
          raw,
          version === undefined ? {} : { 'if-match': version },
        ),
      )
      return {
        settings: projectSettingsSchema.parse(body.settings),
        version: typeof body.version === 'string' ? body.version : null,
      }
    },

    // The skills of every agent for a repository, for the skill picker of the stage prompts.
    async getSkills(repo: string): Promise<SkillList> {
      const body = asRecord(await request('GET', `/repos/${encodeURIComponent(repo)}/skills`))
      return {
        skills: Array.isArray(body.skills) ? body.skills.map(skillInfo) : [],
        notices: Array.isArray(body.notices) ? stringList(body.notices) : [],
      }
    },
  }
}
