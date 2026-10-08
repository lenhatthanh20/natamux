import { LayoutGrid, ListTodo } from 'lucide-react'
import type { ReactNode, Ref } from 'react'
import { listRow } from './ui/styles'
import { SearchField } from './ui/SearchField'

type SidebarProps = {
  search: string
  onSearch: (value: string) => void
  searchRef: Ref<HTMLInputElement>
  tasksCount: number
  agentsCount: number
  tasksActive: boolean
  agentsActive: boolean
  onTasks: () => void
  onAgents: () => void
  // The tasks that wait for the user. Stays in view, so it is not part of the scrolling tree.
  needsYou: ReactNode
  // The workspaces. This is the only part that scrolls.
  tree: ReactNode
  // Under 768 the status items live here, because the status bar is hidden.
  footer?: ReactNode
}

// The left part: filter, the two lists, who needs you, and the tree of workspaces.
export function Sidebar({
  search,
  onSearch,
  searchRef,
  tasksCount,
  agentsCount,
  tasksActive,
  agentsActive,
  onTasks,
  onAgents,
  needsYou,
  tree,
  footer,
}: SidebarProps) {
  return (
    <nav
      aria-label="Workspaces"
      data-part="sidebar"
      className="flex h-full min-h-0 w-full flex-col gap-2 bg-sidebar pt-2"
    >
      <div className="flex flex-col gap-2 px-2">
        <SearchField
          value={search}
          onChange={onSearch}
          inputRef={searchRef}
          placeholder="Filter tasks"
          label="Search tasks"
          hint="/"
        />
        <div className="flex flex-col gap-0.5 text-muted">
          <button type="button" aria-pressed={tasksActive} onClick={onTasks} className={listRow}>
            <ListTodo size={14} aria-hidden="true" />
            <span className="flex-1 text-left">All tasks</span>
            <span className="text-xs">{tasksCount}</span>
          </button>
          <button type="button" aria-pressed={agentsActive} onClick={onAgents} className={listRow}>
            <LayoutGrid size={14} aria-hidden="true" />
            <span className="flex-1 text-left">Agents</span>
            <span className="text-xs">{agentsCount}</span>
          </button>
        </div>
      </div>
      <div className="max-h-[40%] shrink-0 overflow-y-auto px-2">{needsYou}</div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">{tree}</div>
      {footer}
    </nav>
  )
}
