import { z } from 'zod'

// An open issue of the repository on GitHub, as the daemon lists it for import.
export const githubIssueSchema = z.object({
  number: z.number().int().positive(),
  title: z.string(),
  body: z.string(),
  url: z.url(),
  labels: z.array(z.string()),
  // The task made from this issue, when it was imported already.
  taskId: z.string().nullable(),
})

export type GitHubIssue = z.infer<typeof githubIssueSchema>

// The last line of the notes of a task made from an issue. The daemon finds imported issues by it.
export function importedFromLine(url: string): string {
  return `Imported from ${url}`
}
