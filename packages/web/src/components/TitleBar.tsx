import { Menu, PanelLeft, PanelRight, Plus, Search } from 'lucide-react'
import { IconButton } from './ui/IconButton'
import { Kbd } from './ui/Kbd'
import { ICON } from './ui/styles'

type TitleBarProps = {
  sidebarOpen: boolean
  onToggleSidebar: () => void
  // The current view: a workspace shows "repo / T-0018 title", the lists show their own name.
  view: { name: string | null; repo: string | null; taskId: string | null; title: string | null }
  onSearch: () => void
  needsYou: number
  onNeedsYou: () => void
  onNewWorkspace: () => void
  detailsOpen: boolean
  detailsDisabledReason?: string
  onToggleDetails: () => void
  // Under 768: a taller bar, a menu button that carries the connection state (the status bar is
  // hidden there) and a search icon instead of the command center.
  phone: boolean
  liveOk: boolean
}

function Crumb({ view }: Pick<TitleBarProps, 'view'>) {
  if (view.taskId === null) {
    return <span className="truncate font-medium text-ink">{view.name}</span>
  }
  return (
    <span className="flex min-w-0 items-baseline gap-1.5 text-ink-2">
      {view.repo !== null && (
        <span className="max-lg:hidden">
          {view.repo}
          <span aria-hidden="true" className="px-1.5 text-faint">
            /
          </span>
        </span>
      )}
      <span className="shrink-0 font-mono text-xs">{view.taskId}</span>
      {view.title !== null && <span className="truncate max-lg:hidden">{view.title}</span>}
    </span>
  )
}

// The top bar: where you are, how many tasks wait for you, and the few actions that are always
// there. In Electron it will also be the window's drag area, see the hm-titlebar rules.
export function TitleBar({
  sidebarOpen,
  onToggleSidebar,
  view,
  onSearch,
  needsYou,
  onNeedsYou,
  onNewWorkspace,
  detailsOpen,
  detailsDisabledReason,
  onToggleDetails,
  phone,
  liveOk,
}: TitleBarProps) {
  return (
    <header
      className={`hm-titlebar flex shrink-0 items-center gap-2 border-b border-line bg-shell px-2 text-sm text-ink-2 ${phone ? 'h-11' : 'h-9'}`}
    >
      <div className="flex min-w-0 flex-1 basis-0 items-center gap-1.5">
        <span className="relative shrink-0">
          {phone ? (
            <>
              <IconButton
                label="Open the menu"
                icon={Menu}
                tipSide="bottom"
                onClick={onToggleSidebar}
              />
              <span
                aria-hidden="true"
                className={`pointer-events-none absolute top-1.5 right-1.5 size-1.5 rounded-full ${liveOk ? 'bg-add' : 'bg-signal'}`}
              />
            </>
          ) : (
            <IconButton
              label={sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
              icon={PanelLeft}
              shortcut="Mod+B"
              tipSide="bottom"
              aria-pressed={sidebarOpen}
              onClick={onToggleSidebar}
            />
          )}
        </span>
        <h1 className="shrink-0 font-semibold text-ink max-lg:hidden">Helmsman</h1>
        <div className={`flex min-w-0 items-center ${phone ? '' : 'max-w-[40%]'}`}>
          <Crumb view={view} />
        </div>
      </div>

      {!phone && (
        <button
          type="button"
          onClick={onSearch}
          className="flex h-(--control-h) w-[clamp(200px,36vw,520px)] shrink items-center gap-2 rounded-md border border-control bg-input px-2.5 text-muted ring-contrast hover:border-control-hover"
        >
          <Search size={ICON.sm} aria-hidden="true" className="shrink-0" />
          <span className="min-w-0 flex-1 truncate text-left">Search or run a command</span>
          <Kbd shortcut="/" />
        </button>
      )}

      <div className="flex min-w-0 flex-1 basis-0 items-center justify-end gap-1">
        {needsYou > 0 && (
          <button
            type="button"
            onClick={onNeedsYou}
            className={`flex shrink-0 items-center rounded-full border border-signal-line bg-signal-soft px-2.5 text-xs font-medium text-signal ${phone ? 'h-8' : 'h-6'}`}
          >
            {needsYou} {needsYou === 1 ? 'needs' : 'need'} you
          </button>
        )}
        {phone && <IconButton label="Search" icon={Search} tipSide="bottom" onClick={onSearch} />}
        <IconButton
          label="New workspace"
          icon={Plus}
          shortcut="c"
          tipSide="bottom"
          onClick={onNewWorkspace}
        />
        <IconButton
          label={detailsDisabledReason ?? (detailsOpen ? 'Hide details' : 'Show details')}
          icon={PanelRight}
          shortcut="Mod+Alt+B"
          tipSide="bottom"
          aria-pressed={detailsOpen}
          aria-disabled={detailsDisabledReason === undefined ? undefined : true}
          onClick={detailsDisabledReason === undefined ? onToggleDetails : undefined}
        />
      </div>
    </header>
  )
}
