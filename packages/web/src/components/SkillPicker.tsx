import type { SkillList } from '@natamux/core'
import { CircleAlert, Info, X } from 'lucide-react'
import { useEffect, useId, useRef, useState } from 'react'
import { filterSkills, SKILL_GROUPS, skillAgents, skillWarnings } from '../lib/skill-picker'
import { IconButton } from './ui/IconButton'
import { SearchField } from './ui/SearchField'
import { ICON, sectionTitle } from './ui/styles'

type SkillPickerProps = {
  list: SkillList
  onPick: (name: string) => void
  onClose: () => void
}

// The skills of every agent, merged by name. A prompt names a skill as `{skill:<name>}`, never in
// the syntax of one agent, since the stage runs with whatever agent the task has.
export function SkillPicker({ list, onPick, onClose }: SkillPickerProps) {
  const [query, setQuery] = useState('')
  const search = useRef<HTMLInputElement>(null)
  const idPrefix = useId()
  useEffect(() => {
    search.current?.focus()
  }, [])
  const shown = filterSkills(list.skills, query)

  return (
    <fieldset
      aria-label="Skills"
      className="flex min-w-0 flex-col gap-2 rounded-lg border border-line-strong bg-popover p-2 shadow-pop"
    >
      <div className="flex items-center gap-1">
        <SearchField
          value={query}
          onChange={setQuery}
          label="Search skills"
          placeholder="Search skills"
          inputRef={search}
          className="flex-1"
        />
        <IconButton label="Close the skills" icon={X} onClick={onClose} />
      </div>
      {list.notices.map((notice) => (
        <p key={notice} className="m-0 flex items-start gap-1.5 text-xs text-muted">
          <Info size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
          {notice}
        </p>
      ))}
      <div className="flex max-h-72 flex-col gap-3 overflow-y-auto">
        {SKILL_GROUPS.map(({ origin, label }) => {
          const own = shown.filter((skill) => skill.origin === origin)
          if (own.length === 0) {
            return null
          }
          return (
            <section key={origin} aria-label={label} className="flex flex-col gap-0.5">
              <h4 className={`${sectionTitle} m-0 px-1.5`}>{label}</h4>
              <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
                {own.map((skill) => {
                  const about = `${idPrefix}-${skill.name}`
                  return (
                    <li key={skill.name}>
                      <button
                        type="button"
                        aria-label={skill.name}
                        aria-describedby={about}
                        onClick={() => onPick(skill.name)}
                        className="focus-inset flex w-full flex-col gap-0.5 rounded-md px-1.5 py-1 text-left hover:bg-hover"
                      >
                        <span className="font-mono text-xs text-ink">{skill.name}</span>
                        <span id={about} className="flex flex-col gap-0.5 text-xs">
                          {skill.description !== '' && (
                            <span className="text-ink-2">{skill.description}</span>
                          )}
                          <span className="text-muted">{skillAgents(skill)}</span>
                          {skillWarnings(skill).map((warning) => (
                            <span key={warning} className="flex items-start gap-1 text-signal">
                              <CircleAlert
                                size={ICON.xs}
                                aria-hidden="true"
                                className="mt-0.5 shrink-0"
                              />
                              {warning}
                            </span>
                          ))}
                        </span>
                      </button>
                    </li>
                  )
                })}
              </ul>
            </section>
          )
        })}
        {shown.length === 0 && (
          <p className="m-0 px-1.5 text-muted">No skill matches “{query}”.</p>
        )}
      </div>
    </fieldset>
  )
}
