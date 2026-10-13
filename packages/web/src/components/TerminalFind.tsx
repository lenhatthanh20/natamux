import type { SearchAddon } from '@xterm/addon-search'
import { CaseSensitive, ChevronDown, ChevronUp, Regex, X } from 'lucide-react'
import { type KeyboardEvent, useEffect, useMemo, useState } from 'react'
import { type FindResults, findCountLabel } from '../lib/terminal-find'
import { findDecorations } from '../lib/terminal-theme'
import { useResolvedTheme } from '../lib/theme'
import { IconButton } from './ui/IconButton'
import { fieldBox } from './ui/styles'

type TerminalFindProps = {
  // Gives the terminal's addon, made at the first call (TerminalSession.search).
  search: () => SearchAddon
  onClose: () => void
}

// The find bar of a terminal: 28px, at the top right. Enter and Shift+Enter step through the
// matches, Escape closes it and gives the keys back to the shell.
export function TerminalFind({ search: searchOf, onClose }: TerminalFindProps) {
  const search = useMemo(() => searchOf(), [searchOf])
  const [term, setTerm] = useState('')
  const [caseSensitive, setCaseSensitive] = useState(false)
  const [regex, setRegex] = useState(false)
  const [results, setResults] = useState<FindResults>(null)
  const theme = useResolvedTheme()

  const options = { caseSensitive, regex, decorations: findDecorations(theme) }

  useEffect(() => {
    const listener = search.onDidChangeResults(({ resultIndex, resultCount }) =>
      setResults({ index: resultIndex, count: resultCount }),
    )
    return () => {
      listener.dispose()
      search.clearDecorations()
    }
  }, [search])

  // Searching again as the user types or flips an option keeps the current match where it is.
  // biome-ignore lint/correctness/useExhaustiveDependencies: options is built from these values.
  useEffect(() => {
    if (term === '') {
      search.clearDecorations()
      setResults(null)
      return
    }
    search.findNext(term, { ...options, incremental: true })
  }, [term, caseSensitive, regex, search])

  const step = (backwards: boolean) => {
    if (term === '') {
      return
    }
    if (backwards) {
      search.findPrevious(term, options)
    } else {
      search.findNext(term, options)
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      step(event.shiftKey)
    } else if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      onClose()
    }
  }

  const label = findCountLabel(term, results)
  return (
    <search
      aria-label="Find in terminal"
      className="absolute top-2 right-3 z-10 flex items-center gap-0.5 rounded-lg border border-line-strong bg-popover p-0.5 shadow-pop"
    >
      <div className={`${fieldBox} h-6 w-44 border-0 px-1.5 hover:border-0`}>
        <input
          // biome-ignore lint/a11y/noAutofocus: the bar opens because the user asked to search.
          autoFocus
          aria-label="Find"
          placeholder="Find"
          value={term}
          onChange={(event) => setTerm(event.target.value)}
          onKeyDown={onKeyDown}
          className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-placeholder"
        />
        <span role="status" className="shrink-0 text-xs whitespace-nowrap text-muted">
          {label}
        </span>
      </div>
      <IconButton
        label="Match case"
        icon={CaseSensitive}
        size="xs"
        aria-pressed={caseSensitive}
        onClick={() => setCaseSensitive((value) => !value)}
      />
      <IconButton
        label="Use regular expression"
        icon={Regex}
        size="xs"
        aria-pressed={regex}
        onClick={() => setRegex((value) => !value)}
      />
      <IconButton
        label="Previous match"
        icon={ChevronUp}
        size="xs"
        shortcut="Shift+Enter"
        onClick={() => step(true)}
      />
      <IconButton
        label="Next match"
        icon={ChevronDown}
        size="xs"
        shortcut="Enter"
        onClick={() => step(false)}
      />
      <IconButton label="Close find" icon={X} size="xs" shortcut="Esc" onClick={onClose} />
    </search>
  )
}
