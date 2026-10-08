import { useState } from 'react'
import { type Command, sheetGroups } from '../lib/commands'
import { formatShortcut } from '../lib/keys'
import { Dialog } from './ui/Dialog'
import { Kbd } from './ui/Kbd'
import { SearchField } from './ui/SearchField'
import { sectionTitle } from './ui/styles'

function matches(command: Command, query: string): boolean {
  const text = [command.title, ...(command.shortcuts ?? []).map((key) => formatShortcut(key))]
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((word) => text.some((part) => part.toLowerCase().includes(word)))
}

// Every key of the app, from the command registry (UI spec 4.18).
export function ShortcutSheet({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState('')
  const groups = sheetGroups()
    .map(({ group, commands }) => ({
      group,
      commands: commands.filter((command) => matches(command, query)),
    }))
    .filter(({ commands }) => commands.length > 0)

  return (
    <Dialog title="Keyboard shortcuts" size="lg" onClose={onClose}>
      <div className="flex flex-col gap-4 pb-2">
        <SearchField
          value={query}
          onChange={setQuery}
          label="Search the shortcuts"
          placeholder="Search the shortcuts"
        />
        {groups.length === 0 && <p className="text-muted">No shortcut matches “{query}”.</p>}
        <div className="gap-x-8 gap-y-4 lg:columns-2">
          {groups.map(({ group, commands }) => (
            <section
              key={group}
              aria-label={group}
              className="mb-4 flex break-inside-avoid flex-col gap-1"
            >
              <h3 className={sectionTitle}>{group}</h3>
              <ul className="flex flex-col">
                {commands.map((command) => (
                  <li
                    key={command.id}
                    className="flex min-h-7 items-center justify-between gap-4 border-b border-line py-0.5 last:border-b-0"
                  >
                    <span>{command.title}</span>
                    <span className="flex shrink-0 items-center gap-1">
                      {command.shortcuts?.map((shortcut) => (
                        <Kbd key={shortcut} shortcut={shortcut} />
                      ))}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </Dialog>
  )
}
