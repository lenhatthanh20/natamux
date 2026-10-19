import { type CreateIssueWhen, stageName } from '@natamux/core'
import type { LoadedSettings } from '../../lib/api'
import { formatSettings, type ShellField } from '../stages/editor/pipeline-editor'

export const CREATE_ISSUE_OPTIONS: readonly { value: CreateIssueWhen; label: string }[] = [
  { value: 'manual', label: "Only when you click Create issue in the task's Progress view" },
  {
    value: 'on-spec-approved',
    label:
      'Automatically, when you approve the Spec stage. Without a Spec stage, when the pull request opens',
  },
  { value: 'on-pr', label: 'Automatically, just before the pull request opens' },
  { value: 'never', label: 'Never' },
]

// A settings file is a few KiB. A bigger pick is the wrong file, and would slow the textarea.
export const MAX_IMPORT_BYTES = 1024 * 1024

// What the screen started from: the text of the file and its version. A broken file starts from
// the defaults.
export type Base = {
  text: string
  version: string | undefined
  broken: string | null
  path: string
}

export function baseOf(loaded: LoadedSettings): Base {
  const text =
    loaded.error !== null
      ? '{}'
      : (loaded.text ?? (loaded.found ? formatSettings(loaded.settings) : '{}'))
  return { text, version: loaded.version ?? undefined, broken: loaded.error, path: loaded.path }
}

const SHELL_FIELD_LABEL: Record<ShellField['field'], string> = {
  check: 'Check command',
  command: 'Command',
  args: 'Arguments',
  run: 'Condition command',
}

// The import question. A file with commands lists each one in full, since they run on this
// computer once a task reaches the stage.
export function importQuestion(
  name: string,
  dirty: boolean,
  fields: readonly ShellField[],
): string | null {
  const replace = `Replace your changes with ${name}?`
  const later = 'Nothing is saved until you click Save.'
  if (fields.length === 0) {
    return dirty ? `${replace} ${later}` : null
  }
  const list = fields
    .map(
      ({ stage, field, value }) =>
        `${stageName(stage)} (${stage.id}) · ${SHELL_FIELD_LABEL[field]}: ${value}`,
    )
    .join('\n')
  const intro = dirty ? replace : `Import ${name}?`
  return `${intro} It has commands that run in a shell on this computer:\n\n${list}\n\n${later}`
}

export function countLabel(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? '' : 's'}`
}
