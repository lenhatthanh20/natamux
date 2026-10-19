import { type CreateIssueWhen, DEFAULT_PROJECT_SETTINGS, type ProjectSettings } from '@natamux/core'
import { tabPanelProps } from '../../components/ui/Tabs'
import { CREATE_ISSUE_OPTIONS } from './settings-text'

type IssuesTabProps = {
  prefix: string
  tracker: ProjectSettings['tracker']
  onChange: (value: CreateIssueWhen) => void
}

export function IssuesTab({ prefix, tracker, onChange }: IssuesTabProps) {
  return (
    <div {...tabPanelProps(prefix, 'issues', 'Issues')} className="flex flex-col gap-3">
      <fieldset className="flex min-w-0 flex-col gap-1.5">
        <legend className="mb-1.5 text-sm font-semibold">Open a GitHub issue for a task</legend>
        {CREATE_ISSUE_OPTIONS.map((option) => (
          <div key={option.value} className="flex min-h-7 flex-wrap items-center gap-2">
            <label className="flex items-start gap-2 text-sm">
              <input
                type="radio"
                name={`${prefix}-create-issue`}
                value={option.value}
                checked={tracker.createIssue === option.value}
                disabled={tracker.type === 'none'}
                onChange={() => onChange(option.value)}
                className="mt-0.5 size-3.5 shrink-0 accent-accent"
              />
              <span>{option.label}</span>
            </label>
            {option.value === DEFAULT_PROJECT_SETTINGS.tracker.createIssue && (
              <span className="rounded bg-badge px-1.5 text-2xs font-medium text-muted">
                Default
              </span>
            )}
          </div>
        ))}
      </fieldset>
      {tracker.type === 'none' && (
        <p className="m-0 text-xs text-signal">Issues are off: tracker.type is none (JSON tab)</p>
      )}
      <ul
        aria-label="Where the issue comes from"
        className="m-0 flex list-disc flex-col gap-0.5 pl-4 text-xs text-muted"
      >
        <li>
          Opened with <code>gh issue create</code> on GitHub, in this project.
        </li>
        <li>Title: the task title.</li>
        <li>
          Body: the <code>spec.md</code> of the task, or the task notes without one, and a line
          "Made by Natamux for T-0001."
        </li>
      </ul>
    </div>
  )
}
