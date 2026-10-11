import { type ReactNode, type RefObject, useEffect, useRef } from 'react'
import { useAppEvent } from '../lib/app-events'
import { focusKind } from '../lib/dispatch'
import type { DetailsMode, LayoutMode } from '../lib/layout-mode'
import { ResizeHandle } from './ui/ResizeHandle'

export type PanelSizing = {
  width: number
  min: number
  max: number
  defaultWidth: number
  setWidth: (width: number) => void
}

type AppShellProps = {
  layout: LayoutMode
  titleBar: ReactNode
  // Null while the status bar is hidden (under 768).
  statusBar: ReactNode
  sidebar: {
    sizing: PanelSizing
    // The docked sidebar. It is not drawn when the user collapsed it.
    open: boolean
    drawerOpen: boolean
    onCollapse: () => void
    onCloseDrawer: () => void
    content: ReactNode
  }
  // Null when no details are open.
  details: {
    sizing: PanelSizing
    onClose: () => void
    render: (mode: DetailsMode) => ReactNode
  } | null
  children: ReactNode
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
const PART_ORDER = ['sidebar', 'main', 'details', 'status']

// A layer open over the page (the drawer or a sheet): focus moves in, Escape closes it, and
// focus goes back to where it was. A drawer with a scrim also keeps Tab inside.
// The drawer takes focus itself, not its first field, so a phone's keyboard does not pop up.
function useLayer(
  ref: RefObject<HTMLElement | null>,
  onClose: () => void,
  trap: boolean,
  focusPanel = false,
) {
  const close = useRef(onClose)
  close.current = onClose
  useEffect(() => {
    const element = ref.current
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    if (focusPanel) {
      element?.focus()
    } else {
      element?.querySelector<HTMLElement>(FOCUSABLE)?.focus()
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        // Beside a layer with no scrim the terminal still takes keys, and Esc means something to
        // the agent or the editor in it.
        if (!trap && focusKind(event.target) === 'terminal') {
          return
        }
        // A menu, a dialog or a tooltip on top of this layer takes the key first.
        const above = [
          ...document.querySelectorAll('[role="dialog"], [role="menu"], [role="tooltip"]'),
        ].some((layer) => layer !== element)
        if (above) {
          return
        }
        event.stopPropagation()
        close.current()
        return
      }
      if (event.key !== 'Tab' || !trap || element === null) {
        return
      }
      const stops = [...element.querySelectorAll<HTMLElement>(FOCUSABLE)]
      const first = stops[0]
      const last = stops[stops.length - 1]
      if (first === undefined || last === undefined) {
        return
      }
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => {
      window.removeEventListener('keydown', onKeyDown, true)
      // The page may have moved focus on already, for example into a new terminal.
      if (document.activeElement === document.body) {
        // The opener may be gone or may never have had the focus (a key opened the layer).
        const target =
          previous?.isConnected && previous !== document.body
            ? previous
            : document.getElementById('main')
        target?.focus()
      }
    }
  }, [ref, trap, focusPanel])
}

function Drawer({
  phone,
  onClose,
  children,
}: {
  phone: boolean
  onClose: () => void
  children: ReactNode
}) {
  const panel = useRef<HTMLDivElement>(null)
  useLayer(panel, onClose, true, true)
  return (
    <div className="fixed inset-0 z-(--z-sheet) flex">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close the sidebar"
        onClick={onClose}
        className="hm-scrim absolute inset-0 cursor-default bg-scrim"
      />
      <div
        ref={panel}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label="Sidebar"
        className={`hm-slide-left relative h-full focus:outline-none border-r border-line-strong bg-sidebar shadow-modal ${phone ? 'w-[min(360px,100vw)]' : 'w-[min(300px,85vw)]'}`}
      >
        {children}
      </div>
    </div>
  )
}

function Sheet({
  scrim,
  onClose,
  children,
}: {
  scrim: boolean
  onClose: () => void
  children: ReactNode
}) {
  const panel = useRef<HTMLDivElement>(null)
  useLayer(panel, onClose, scrim)
  return (
    <div
      className={`absolute inset-0 z-(--z-sheet) flex justify-end ${scrim ? '' : 'pointer-events-none'}`}
    >
      {scrim && (
        <button
          type="button"
          tabIndex={-1}
          aria-label="Close the details"
          onClick={onClose}
          className="hm-scrim absolute inset-0 cursor-default bg-scrim"
        />
      )}
      <div
        ref={panel}
        {...(scrim ? { role: 'dialog', 'aria-modal': true, 'aria-label': 'Details' } : {})}
        className="hm-slide-right pointer-events-auto relative h-full w-[min(360px,90vw)] border-l border-line-strong bg-surface shadow-modal"
      >
        {children}
      </div>
    </div>
  )
}

// F6 and Shift+F6 move between the parts: sidebar, main, details, status bar. The keys are
// commands, see lib/commands.ts.
function usePartCycling(root: RefObject<HTMLElement | null>) {
  useAppEvent<number>('focus-part', (step) => {
    const parts = [...(root.current?.querySelectorAll<HTMLElement>('[data-part]') ?? [])].sort(
      (a, b) => PART_ORDER.indexOf(a.dataset.part ?? '') - PART_ORDER.indexOf(b.dataset.part ?? ''),
    )
    if (parts.length === 0) {
      return
    }
    const at = parts.findIndex((part) => part.contains(document.activeElement))
    const target =
      parts[
        (at === -1 ? (step === 1 ? 0 : parts.length - 1) : at + step + parts.length) % parts.length
      ]
    if (target === undefined) {
      return
    }
    const inner = target.querySelector<HTMLElement>(FOCUSABLE)
    if (inner !== null) {
      inner.focus()
    } else {
      target.tabIndex = -1
      target.focus()
    }
  })
}

// The frame of the app, section 3.1 of the UI spec: title bar, sidebar, main, details and status
// bar, with the sidebar and the details changing form as the window narrows.
export function AppShell({
  layout,
  titleBar,
  statusBar,
  sidebar,
  details,
  children,
}: AppShellProps) {
  const root = useRef<HTMLDivElement>(null)
  usePartCycling(root)
  const phone = layout.details === 'page'

  const detailsMode = details === null ? null : layout.details
  const page = detailsMode === 'page'
  // What a drawer or a sheet with a scrim covers cannot be reached by key or by a screen reader.
  // The title bar stays live: its buttons close the layer.
  const covered =
    (layout.sidebar === 'drawer' && sidebar.drawerOpen) ||
    (detailsMode === 'sheet' && layout.compact)

  return (
    <div ref={root} className="flex h-dvh flex-col overflow-hidden">
      <a
        href="#main"
        className="sr-only rounded-md bg-popover px-3 py-1.5 text-ink shadow-pop focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-(--z-tooltip)"
      >
        Skip to main content
      </a>
      {titleBar}
      <div className="relative flex min-h-0 flex-1">
        {layout.sidebar === 'docked' && sidebar.open && (
          <>
            <div
              inert={covered}
              style={{ width: sidebar.sizing.width }}
              className="h-full shrink-0 bg-sidebar"
            >
              {sidebar.content}
            </div>
            <ResizeHandle
              label="Resize the sidebar"
              edge="right"
              width={sidebar.sizing.width}
              min={sidebar.sizing.min}
              max={sidebar.sizing.max}
              defaultWidth={sidebar.sizing.defaultWidth}
              collapseBelow={180}
              onCollapse={sidebar.onCollapse}
              onResize={sidebar.sizing.setWidth}
            />
          </>
        )}
        {layout.sidebar === 'drawer' && sidebar.drawerOpen && (
          <Drawer phone={phone} onClose={sidebar.onCloseDrawer}>
            {sidebar.content}
          </Drawer>
        )}

        <main
          id="main"
          data-part="main"
          tabIndex={-1}
          inert={covered}
          className={`@container/main flex min-w-0 flex-1 flex-col overflow-hidden bg-canvas focus:outline-none ${page ? 'hidden' : ''}`}
        >
          {children}
        </main>

        {details !== null && detailsMode === 'docked' && (
          <>
            <ResizeHandle
              label="Resize the details"
              edge="left"
              width={details.sizing.width}
              min={details.sizing.min}
              max={details.sizing.max}
              defaultWidth={details.sizing.defaultWidth}
              onResize={details.sizing.setWidth}
            />
            <div
              data-part="details"
              style={{ width: details.sizing.width }}
              className="h-full shrink-0 bg-surface"
            >
              {details.render('docked')}
            </div>
          </>
        )}
        {details !== null && detailsMode === 'sheet' && (
          <Sheet scrim={layout.compact} onClose={details.onClose}>
            <div data-part="details" className="h-full">
              {details.render('sheet')}
            </div>
          </Sheet>
        )}
        {details !== null && page && (
          <div data-part="details" className="h-full min-w-0 flex-1 bg-surface">
            {details.render('page')}
          </div>
        )}
      </div>
      {statusBar !== null && <div data-part="status">{statusBar}</div>}
    </div>
  )
}
