import { type GitHubIssue, importedFromLine, type Task } from '@natamux/core'
import { GitHubError, type RunGh, runGh } from '../git/gh.js'
import { Jobs } from '../shared/jobs.js'
import type { TaskService } from '../tasks/task-service.js'

export const MAX_ISSUES = 100
const TITLE_LIMIT = 200

type IssueListItem = {
  number?: number
  title?: string
  body?: string
  url?: string
  labels?: { name?: string }[]
}

// Reads `gh issue list --json number,title,body,url,labels`.
export function parseIssueList(output: string): Omit<GitHubIssue, 'taskId'>[] {
  let items: IssueListItem[]
  try {
    items = JSON.parse(output) as IssueListItem[]
  } catch {
    throw new GitHubError(`could not read the issue list: ${output.slice(0, 200)}`)
  }
  return items.flatMap((item) =>
    typeof item.number === 'number' && item.url
      ? [
          {
            number: item.number,
            title: item.title ?? '',
            body: item.body ?? '',
            url: item.url,
            labels: (item.labels ?? []).flatMap((label) => (label.name ? [label.name] : [])),
          },
        ]
      : [],
  )
}

type IssueImporterOptions = {
  tasks: TaskService
  // The folder of a repository (null: the default one), so gh knows which repository to ask.
  repoPath: (repo: string | null) => string
  gh?: RunGh
  jobs?: Jobs
}

// Turns open GitHub issues into draft tasks. An issue becomes a task once.
export class IssueImporter {
  private readonly tasks: TaskService
  private readonly repoPath: (repo: string | null) => string
  private readonly gh: RunGh
  private readonly jobs: Jobs

  constructor(options: IssueImporterOptions) {
    this.tasks = options.tasks
    this.repoPath = options.repoPath
    this.gh = options.gh ?? runGh
    this.jobs = options.jobs ?? new Jobs()
  }

  async list(repo: string | null = null): Promise<GitHubIssue[]> {
    const output = await this.gh(this.repoPath(repo), [
      'issue',
      'list',
      '--state',
      'open',
      '--limit',
      String(MAX_ISSUES),
      '--json',
      'number,title,body,url,labels',
    ])
    const imported = this.importedTaskIds()
    return parseIssueList(output).map((issue) => ({
      ...issue,
      taskId: imported.get(issue.url) ?? null,
    }))
  }

  // Makes a task for each issue number that is open and not imported yet.
  async import(numbers: readonly number[], repo: string | null = null): Promise<Task[]> {
    return this.jobs.run({ kind: 'issue-import', label: 'Importing issues', taskId: null }, () =>
      this.importNow(numbers, repo),
    )
  }

  private async importNow(numbers: readonly number[], repo: string | null): Promise<Task[]> {
    const wanted = new Set(numbers)
    const issues = (await this.list(repo)).filter(
      (issue) => wanted.has(issue.number) && issue.taskId === null,
    )
    return issues.map((issue) =>
      this.tasks.createTask({
        title: (issue.title.trim() || `Issue #${issue.number}`).slice(0, TITLE_LIMIT),
        description: [issue.body.trim(), importedFromLine(issue.url)]
          .filter((part) => part !== '')
          .join('\n\n'),
        labels: issue.labels,
        repo,
      }),
    )
  }

  // Issue address to task id, read from the last line of each task's notes.
  private importedTaskIds(): Map<string, string> {
    const found = new Map<string, string>()
    for (const task of this.tasks.listTasks()) {
      const match = /Imported from (\S+)\s*$/.exec(task.description)
      if (match?.[1]) {
        found.set(match[1], task.id)
      }
    }
    return found
  }
}
