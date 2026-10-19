import {
  PROMPT_PLACEHOLDERS,
  type SkillList,
  type Stage,
  type Step,
  skillPlaceholder,
} from '@natamux/core'
import type { UseQueryResult } from '@tanstack/react-query'
import { Sparkles } from 'lucide-react'
import { useEffect, useId, useRef } from 'react'
import { SkillPicker } from '../../../components/SkillPicker'
import { Button } from '../../../components/ui/Button'
import { textarea } from '../../../components/ui/styles'
import { setText, shownDefaultPrompt } from './pipeline-editor'
import { DefaultMark, Field, useLocalText } from './StageField'

function skillsReason(
  skills: Pick<UseQueryResult<SkillList, Error>, 'data' | 'error' | 'isPending'>,
): string | undefined {
  if (skills.data !== undefined) {
    return skills.data.skills.length === 0
      ? 'No agent has a skill in this project or on this machine'
      : undefined
  }
  if (skills.error !== null) {
    return `Could not read the skills: ${skills.error.message}`
  }
  return skills.isPending ? 'Reading the skills of the agents…' : undefined
}

export function PromptField({
  stage,
  steps,
  skills,
  skillsOpen,
  onSkillsOpen,
  onChange,
}: {
  stage: Stage
  steps: readonly Step[]
  skills: Pick<UseQueryResult<SkillList, Error>, 'data' | 'error' | 'isPending'>
  skillsOpen: boolean
  onSkillsOpen: (open: boolean) => void
  onChange: (steps: Step[]) => void
}) {
  const id = useId()
  const area = useRef<HTMLTextAreaElement>(null)
  // Where the cursor was, for a placeholder or a skill picked with a button.
  const cursor = useRef<{ start: number; end: number } | null>(null)
  const custom = stage.prompt !== undefined
  const text = stage.prompt ?? shownDefaultPrompt(stage)
  const prompt = useLocalText(text, (value) => onChange(setText(steps, stage.id, 'prompt', value)))
  const remember = () => {
    const element = area.current
    if (element !== null) {
      cursor.current = { start: element.selectionStart, end: element.selectionEnd }
    }
  }
  const insert = (token: string) => {
    const { start, end } = cursor.current ?? { start: text.length, end: text.length }
    onChange(setText(steps, stage.id, 'prompt', text.slice(0, start) + token + text.slice(end)))
    const at = start + token.length
    cursor.current = { start: at, end: at }
    requestAnimationFrame(() => {
      area.current?.focus()
      area.current?.setSelectionRange(at, at)
    })
  }
  const reason = skillsReason(skills)
  // A picker closed with Escape takes the focus with it; the button gets it back.
  const skillButton = useRef<HTMLButtonElement>(null)
  const wasOpen = useRef(skillsOpen)
  useEffect(() => {
    if (wasOpen.current && !skillsOpen && document.activeElement === document.body) {
      skillButton.current?.focus()
    }
    wasOpen.current = skillsOpen
  }, [skillsOpen])

  return (
    <Field
      label="Prompt"
      htmlFor={`${id}-prompt`}
      mark={
        <DefaultMark
          isDefault={!custom}
          onReset={() => onChange(setText(steps, stage.id, 'prompt', ''))}
        />
      }
      help={
        custom
          ? 'A prompt of your own gets no commit rule. Say whether the agent should commit.'
          : undefined
      }
    >
      <div className="flex flex-wrap items-center gap-1">
        {PROMPT_PLACEHOLDERS.map((placeholder) => (
          <button
            key={placeholder}
            type="button"
            aria-label={`Insert {${placeholder}}`}
            onClick={() => insert(`{${placeholder}}`)}
            className="inline-flex h-(--control-h-xs) items-center rounded-md border border-line bg-raised px-1.5 font-mono text-xs text-ink-2 hover:border-control-hover hover:text-ink"
          >
            {`{${placeholder}}`}
          </button>
        ))}
        <span className="flex-1" />
        <Button
          ref={skillButton}
          size="xs"
          icon={Sparkles}
          aria-expanded={skillsOpen}
          disabledReason={reason}
          onClick={() => onSkillsOpen(!skillsOpen)}
        >
          Insert skill
        </Button>
      </div>
      {skillsOpen && skills.data !== undefined && reason === undefined && (
        <SkillPicker
          list={skills.data}
          onClose={() => onSkillsOpen(false)}
          onPick={(name) => {
            onSkillsOpen(false)
            insert(skillPlaceholder(name))
          }}
        />
      )}
      <textarea
        ref={area}
        id={`${id}-prompt`}
        {...prompt}
        onBlur={() => {
          remember()
          prompt.onBlur()
        }}
        onSelect={remember}
        rows={9}
        spellCheck={false}
        className={`${textarea} w-full resize-y font-mono text-xs leading-5`}
      />
      <p className="m-0 font-mono text-xs text-muted">
        When you are done, run <code>natamux done</code>, unless the steps above say not to.
      </p>
    </Field>
  )
}
