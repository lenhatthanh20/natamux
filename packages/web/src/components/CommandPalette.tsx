import type { Task } from '@helmsman/core'
import { useQueries } from '@tanstack/react-query'
import { FolderGit2, Plus, Search, SquareTerminal, X } from 'lucide-react'
import {
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from 'react'
import { createPortal } from 'react-dom'
import { type Command, type CommandContext, paletteTitle } from '../lib/commands'
import { type Piece, type Range, splitByRanges } from '../lib/fuzzy'
import {
  allSections,
  digitOf,
  flatRows,
  jumpSections,
  type PaletteRow,
  type PaletteScope,
  type PaletteSection,
  type RepoItem,
} from '../lib/palette'
import { formatAge } from '../lib/tasks'
import { StatusGlyph } from './StatusGlyph'
import { lockPage } from './ui/Dialog'
import { Kbd } from './ui/Kbd'
import { ICON } from './ui/styles'

export type PaletteMode = 'all' | 'jump'

type CommandPaletteProps = {
  mode: PaletteMode
  initialScope?: PaletteScope
  initialQuery?: string
  tasks: readonly Task[]
  repos: readonly RepoItem[]
  recent: readonly string[]
  now: Date
  context: CommandContext
  loadTerminals: (taskId: string) => Promise<string[]>
  onClose: () => void
  onOpenTask: (taskId: string, shell?: string) => void
  onAddToAgents: (taskId: string) => void
  onRunCommand: (command: Command) => void
  onCreateTask: (title: string) => void
  onNewWorkspace: (options: { title?: string; repo?: string }) => void
}

function Highlighted({ text, ranges }: { text: string; ranges: readonly Range[] }) {
  const pieces: Piece[] = splitByRanges(text, ranges)
  return (
    <>
      {pieces.map((piece, index) =>
        piece.match ? (
          // The pieces of one text never reorder, so the position is the key.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above.
          <b key={index} className="font-semibold text-ink">
            {piece.text}
          </b>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: see above.
          <span key={index}>{piece.text}</span>
        ),
      )}
    </>
  )
}

function RowIcon({ children }: { children: ReactNode }) {
  return (
    <span className="flex size-4 shrink-0 items-center justify-center text-ink-2">{children}</span>
  )
}

function RowBody({ row, now }: { row: PaletteRow; now: Date }) {
  switch (row.kind) {
    case 'command':
      return (
        <>
          <span className="min-w-0 flex-1 truncate">
            <Highlighted text={paletteTitle(row.command)} ranges={row.ranges} />
          </span>
          {row.command.shortcuts?.[0] !== undefined && <Kbd shortcut={row.command.shortcuts[0]} />}
        </>
      )
    case 'task': {
      const { task } = row
      return (
        <>
          <StatusGlyph state={task.state} />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-ink">
              <Highlighted text={task.title} ranges={row.titleRanges} />
            </span>
            <span className="flex min-w-0 items-center gap-2 text-xs text-muted">
              <span className="shrink-0 font-mono">
                <Highlighted text={task.id} ranges={row.idRanges} />
              </span>
              {task.branch !== null && <span className="truncate font-mono">{task.branch}</span>}
            </span>
          </span>
          <span className="shrink-0 self-start pt-0.5 text-xs text-muted">
            {formatAge(task.updatedAt, now)}
          </span>
        </>
      )
    }
    case 'shell':
      return (
        <>
          <RowIcon>
            <SquareTerminal size={ICON.sm} aria-hidden="true" />
          </RowIcon>
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-ink">Shell {row.name}</span>
            <span className="flex min-w-0 items-center gap-2 text-xs text-muted">
              <span className="shrink-0 font-mono">{row.task.id}</span>
              <span className="truncate">{row.task.title}</span>
            </span>
          </span>
        </>
      )
    case 'repo':
      return (
        <>
          <RowIcon>
            <FolderGit2 size={ICON.sm} aria-hidden="true" />
          </RowIcon>
          <span className="min-w-0 flex-1 truncate">
            <Highlighted text={row.repo.name} ranges={row.ranges} />
          </span>
          <span className="shrink-0 text-xs text-muted">New workspace here</span>
        </>
      )
    case 'create-task':
    case 'new-workspace':
      return (
        <>
          <RowIcon>
            <Plus size={ICON.sm} aria-hidden="true" />
          </RowIcon>
          <span className="min-w-0 flex-1 truncate">
            {row.kind === 'create-task' ? 'Create task' : 'New workspace'}{' '}
            <span className="text-ink">“{row.title}”</span>
          </span>
        </>
      )
  }
}

const TWO_LINE: readonly PaletteRow['kind'][] = ['task', 'shell']

function nextScope(scope: PaletteScope): PaletteScope {
  return scope === null ? 'needs-you' : null
}

// The command palette (Mod+K) and the jump palette (Mod+J), UI spec 4.17. The rows are worked out
// from a snapshot taken when it opens, so live updates never move a row under the pointer.
export function CommandPalette({
  mode,
  initialScope = null,
  initialQuery = '',
  tasks,
  repos,
  recent,
  now,
  context,
  loadTerminals,
  onClose,
  onOpenTask,
  onAddToAgents,
  onRunCommand,
  onCreateTask,
  onNewWorkspace,
}: CommandPaletteProps) {
  const [query, setQuery] = useState(initialQuery)
  const [scope, setScope] = useState<PaletteScope>(initialScope)
  const [activeKey, setActiveKey] = useState<string | null>(null)
  const [snapshot] = useState(() => ({ tasks, now, recent }))
  const listId = useId()
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    input.current?.focus()
    const unlock = lockPage()
    return () => {
      unlock()
      previous?.focus()
    }
  }, [])

  // The open shells of each workspace, for the jump palette.
  const workspaces = snapshot.tasks.filter((task) => task.workspace !== null)
  const shellQueries = useQueries({
    queries: workspaces.map((task) => ({
      queryKey: ['terminals', task.id],
      queryFn: () => loadTerminals(task.id),
      enabled: mode === 'jump' && workspaces.length <= 20,
    })),
  })
  const terminals =
    mode === 'jump'
      ? Object.fromEntries(
          workspaces.map((task, index) => [task.id, shellQueries[index]?.data ?? []]),
        )
      : undefined
  const terminalKey = JSON.stringify(terminals)

  // The commands' `when` is read as the palette opens, and terminalKey stands for terminals.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above.
  const sections: PaletteSection[] = useMemo(
    () =>
      mode === 'jump'
        ? jumpSections({
            query,
            tasks: snapshot.tasks,
            recent: snapshot.recent,
            now: snapshot.now,
            scope,
            terminals,
          })
        : allSections({
            query,
            tasks: snapshot.tasks,
            context,
            repos,
            recent: snapshot.recent,
            now: snapshot.now,
            scope,
          }),
    [mode, query, scope, snapshot, repos, terminalKey],
  )
  const rows = flatRows(sections)
  const found = rows.findIndex((row) => row.key === activeKey)
  const index = found === -1 ? 0 : found
  const active = rows[index]

  useEffect(() => {
    if (active !== undefined) {
      document.getElementById(`${listId}-${active.key}`)?.scrollIntoView?.({ block: 'nearest' })
    }
  }, [active, listId])

  const pick = (row: PaletteRow | undefined, shift = false) => {
    if (row === undefined) {
      return
    }
    onClose()
    switch (row.kind) {
      case 'command':
        onRunCommand(row.command)
        return
      case 'task':
        if (shift && row.task.workspace !== null) {
          onAddToAgents(row.task.id)
        } else {
          onOpenTask(row.task.id)
        }
        return
      case 'shell':
        if (shift) {
          onAddToAgents(row.task.id)
        } else {
          onOpenTask(row.task.id, row.name)
        }
        return
      case 'repo':
        onNewWorkspace({ repo: row.repo.id })
        return
      case 'create-task':
        onCreateTask(row.title)
        return
      case 'new-workspace':
        onNewWorkspace({ title: row.title })
    }
  }

  const move = (to: number) => {
    const target = rows[(to + rows.length) % Math.max(rows.length, 1)]
    if (target !== undefined) {
      setActiveKey(target.key)
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return
    }
    const digit = /^Digit([1-9])$/.exec(event.code)?.[1]
    if (event.altKey && digit !== undefined) {
      event.preventDefault()
      pick(rows[Number(digit) - 1])
      return
    }
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault()
        move(index + 1)
        return
      case 'ArrowUp':
        event.preventDefault()
        move(index - 1)
        return
      case 'PageDown':
        event.preventDefault()
        setActiveKey(rows[Math.min(index + 8, rows.length - 1)]?.key ?? null)
        return
      case 'PageUp':
        event.preventDefault()
        setActiveKey(rows[Math.max(index - 8, 0)]?.key ?? null)
        return
      case 'Enter':
        event.preventDefault()
        pick(active, event.shiftKey)
        return
      case 'Tab':
        event.preventDefault()
        setScope(nextScope(scope))
        setActiveKey(null)
        return
      case 'Backspace':
        if (query === '' && scope !== null) {
          event.preventDefault()
          setScope(null)
          setActiveKey(null)
        }
        return
      case 'Escape':
        event.preventDefault()
        event.stopPropagation()
        onClose()
    }
  }

  const empty =
    rows.length === 0
      ? query.trim().startsWith('>')
        ? 'No command matches.'
        : scope === 'needs-you'
          ? 'Nothing needs you.'
          : mode === 'jump'
            ? 'No workspace yet.'
            : 'Nothing to show yet. Type to search.'
      : null

  let position = -1
  return createPortal(
    <div className="fixed inset-0 z-(--z-palette) flex items-start justify-center p-4 pt-[12vh] max-md:p-0">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close the palette"
        data-dialog-close
        onClick={onClose}
        className="hm-scrim absolute inset-0 cursor-default bg-scrim-palette"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={mode === 'jump' ? 'Jump to a workspace' : 'Command palette'}
        className="hm-dialog relative flex w-full max-w-160 flex-col overflow-hidden rounded-xl border border-line-strong bg-popover shadow-modal ring-1 ring-contrast max-md:max-w-none max-md:rounded-t-none"
      >
        <div className="flex h-11 shrink-0 items-center gap-2.5 border-b border-line px-3.5">
          <Search size={ICON.md} aria-hidden="true" className="shrink-0 text-muted" />
          <input
            ref={input}
            type="text"
            role="combobox"
            aria-label={mode === 'jump' ? 'Jump to a workspace' : 'Search or run a command'}
            aria-expanded="true"
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={active === undefined ? undefined : `${listId}-${active.key}`}
            autoComplete="off"
            spellCheck={false}
            value={query}
            placeholder={
              mode === 'jump'
                ? 'Jump to a workspace'
                : 'Search tasks and commands. Type > for commands, # for a task id.'
            }
            onChange={(event) => {
              setQuery(event.target.value)
              setActiveKey(null)
            }}
            onKeyDown={onKeyDown}
            className="min-w-0 flex-1 bg-transparent text-md text-ink outline-none placeholder:text-placeholder"
          />
          {scope !== null && (
            <button
              type="button"
              onClick={() => {
                setScope(null)
                setActiveKey(null)
                input.current?.focus()
              }}
              aria-label="Remove the Needs you scope"
              className="flex h-6 shrink-0 items-center gap-1 rounded-full border border-signal-line bg-signal-soft pr-1.5 pl-2.5 text-xs font-medium text-signal"
            >
              Needs you
              <X size={ICON.xs} aria-hidden="true" />
            </button>
          )}
        </div>

        <div
          id={listId}
          role="listbox"
          aria-label="Results"
          className="max-h-[min(440px,60vh)] overflow-y-auto p-1.5"
        >
          {empty !== null && <p className="px-2.5 py-3 text-muted">{empty}</p>}
          {sections.map((section) => (
            <div key={section.id} role="presentation">
              {section.title !== null && (
                <p className="flex h-6 items-center px-2.5 text-2xs font-semibold tracking-[0.06em] text-muted uppercase">
                  {section.title}
                </p>
              )}
              {section.rows.map((row) => {
                position += 1
                const selected = position === index
                const digit = digitOf(position)
                return (
                  // The keys are the field's: it points at the active row with aria-activedescendant.
                  // biome-ignore lint/a11y/useKeyWithClickEvents: see above.
                  <div
                    key={row.key}
                    id={`${listId}-${row.key}`}
                    role="option"
                    tabIndex={-1}
                    aria-selected={selected}
                    aria-keyshortcuts={digit === null ? undefined : `Alt+${digit}`}
                    onPointerMove={() => setActiveKey(row.key)}
                    onPointerDown={(event) => {
                      // Keeps the focus in the field.
                      event.preventDefault()
                    }}
                    onClick={(event) => pick(row, event.shiftKey)}
                    className={`flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-ink-2 ${
                      TWO_LINE.includes(row.kind) ? 'h-11' : 'h-8'
                    } ${selected ? 'bg-selection-active shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--accent)_40%,transparent)]' : ''}`}
                  >
                    <RowBody row={row} now={snapshot.now} />
                  </div>
                )
              })}
            </div>
          ))}
        </div>

        <div className="flex h-7 shrink-0 items-center gap-4 border-t border-line px-3.5 text-xs text-muted max-md:hidden">
          <span>↑↓ move</span>
          <span>↵ open</span>
          <span>⇧↵ add to Agents</span>
          <span>⌥1-9 jump</span>
          <span>⇥ needs you</span>
          <span className="ml-auto">Esc close</span>
        </div>
      </div>
    </div>,
    document.body,
  )
}
