export type IssueRef = {
  number: number
  // The repository of an issue address: "owner/name", for the closing line of a pull request.
  slug?: string
  // The same for `gh --repo`, with the host when it is not github.com.
  repo?: string
}

const ISSUE_ADDRESS = /^https?:\/\/([^/\s]+)\/([^/\s]+)\/([^/\s]+)\/issues\/(\d+)\/?(?:[?#]\S*)?$/
const ISSUE_KEY = /^(?:([\w.-]+\/[\w.-]+)#|#?)(\d+)$/

// An issue in "#42", "42", "owner/name#42" or an issue address (a comment link too).
export function parseIssueRef(ref: string): IssueRef | undefined {
  const text = ref.trim()
  const address = ISSUE_ADDRESS.exec(text)
  if (address !== null) {
    const [, host = '', owner = '', name = '', number = ''] = address
    const slug = `${owner}/${name}`
    return { number: Number(number), slug, repo: host === 'github.com' ? slug : `${host}/${slug}` }
  }
  const short = ISSUE_KEY.exec(text)
  if (short === null) {
    return undefined
  }
  const [, slug, number = ''] = short
  return slug === undefined
    ? { number: Number(number) }
    : { number: Number(number), slug, repo: slug }
}

// The issue number in "#42", "42" or an issue address.
export function issueNumber(ref: string): number | undefined {
  return parseIssueRef(ref)?.number
}

export type GhIssue = {
  number?: number
  title?: string
  body?: string
  url?: string
  labels?: { name?: string }[]
  comments?: { author?: { login?: string }; body?: string; createdAt?: string }[]
}

export function issueContext(issue: GhIssue): string {
  const labels = (issue.labels ?? []).flatMap((label) => (label.name ? [label.name] : []))
  const comments = (issue.comments ?? []).map((comment) =>
    [
      `### ${comment.author?.login ?? 'someone'}${comment.createdAt ? `, ${comment.createdAt}` : ''}`,
      '',
      (comment.body ?? '').trim(),
    ].join('\n'),
  )
  return [
    `# ${issue.title ?? `Issue #${issue.number ?? ''}`}`,
    '',
    `Source: ${issue.url ?? ''}`,
    ...(labels.length === 0 ? [] : [`Labels: ${labels.join(', ')}`]),
    '',
    (issue.body ?? '').trim() || '(No description.)',
    ...(comments.length === 0 ? [] : ['', '## Comments', '', comments.join('\n\n')]),
    '',
  ].join('\n')
}
