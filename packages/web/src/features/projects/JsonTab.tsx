import type { PipelineIssue } from '@natamux/core'
import { CircleAlert } from 'lucide-react'
import { ICON, textarea } from '../../components/ui/styles'
import { tabPanelProps } from '../../components/ui/Tabs'

type JsonTabProps = {
  prefix: string
  text: string
  // The parse error of the text, or null when it parses.
  error: string | null
  issues: readonly PipelineIssue[]
  onChange: (text: string) => void
}

export function JsonTab({ prefix, text, error, issues, onChange }: JsonTabProps) {
  return (
    <div {...tabPanelProps(prefix, 'json', 'JSON')} className="flex flex-col gap-2">
      <textarea
        value={text}
        onChange={(event) => onChange(event.target.value)}
        rows={22}
        spellCheck={false}
        aria-label=".natamux.json"
        aria-invalid={error !== null}
        className={`${textarea} w-full resize-y font-mono text-xs leading-5`}
      />
      {error !== null && (
        <p role="alert" className="m-0 flex items-start gap-1.5 text-xs text-danger-text">
          <CircleAlert size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
          {error} Fix it to use the Stages tab.
        </p>
      )}
      {issues.length > 0 && (
        <ul className="m-0 flex list-none flex-col gap-0.5 p-0 text-xs">
          {issues.map((issue) => (
            <li
              key={`${issue.stage}-${issue.message}`}
              className={issue.level === 'error' ? 'text-danger-text' : 'text-signal'}
            >
              {issue.stage === null ? issue.message : `${issue.stage}: ${issue.message}`}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
