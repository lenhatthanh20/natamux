import type { FitAddon } from '@xterm/addon-fit'
import type { SearchAddon } from '@xterm/addon-search'
import type { Terminal } from '@xterm/xterm'
import type { ResolvedTheme } from '../../../lib/theme'
import type { GpuRenderer } from '../gpu-renderer'
import type { CachedTerminal } from '../terminal-cache'

export type TerminalStatus = 'connecting' | 'open' | 'reconnecting' | 'ended' | 'error' | 'paused'
export type TerminalSessionState = {
  status: TerminalStatus
  error: string | null
  scrolledUp: boolean
  // The screen is hidden while a large catch-up is written, and the view shows a skeleton.
  catchingUp: boolean
  // From the `exit` frame; null when the shell ended another way or runs.
  exitCode: number | null
  // From the `paused` frame: after how many idle minutes the daemon ended the agent.
  pausedMinutes: number | null
}

// From `snapshot` or `resume` until xterm parsed the last chunk before `replayed` (spec 6).
export type CatchUp = {
  kind: 'snapshot' | 'resume'
  bytes: number
  // `replayed` came: later frames are live output, counted, though xterm may not have parsed the
  // catch-up yet.
  replayed: boolean
  timer: ReturnType<typeof setTimeout>
}

export type TerminalSession = CachedTerminal & {
  readonly terminal: Terminal
  // Made at the first call: most terminals are never searched.
  search(): SearchAddon
  // The same object until something changes, for useSyncExternalStore.
  state(): TerminalSessionState
  subscribe(listener: () => void): () => void
  // takeFocus false keeps the keyboard where it is: of several shells side by side, only one may
  // take it, or the focus jumps from shell to shell as they show.
  show(container: HTMLElement, takeFocus?: boolean): void
  setOnScreen(onScreen: boolean): void
  applySettings(): void
}

// The state that the parts of one session share (index, layout, catch-up, connection).
export type SessionCore = {
  readonly terminal: Terminal
  readonly fit: FitAddon
  readonly host: HTMLDivElement
  state: TerminalSessionState
  theme: ResolvedTheme
  // The font the cells were measured with. Empty means "measure again".
  measuredFont: string
  disposed: boolean
  ended: boolean
  onScreen: boolean
  socket: WebSocket | null
  gpu: GpuRenderer | null
  // Set while a resize is local only: the shell gets the size once the pane settles.
  holdSize: boolean
  // The screen is hidden while the pane changes size (`covered`) and while a catch-up is written
  // (`catchUpHidden`). It shows when neither holds.
  covered: boolean
  // A resize went out while covered, so the shell redraws before the screen shows.
  redrawWanted: boolean
  redrawTimer: ReturnType<typeof setTimeout> | undefined
  catchUp: CatchUp | null
  // Bytes of output received outside catch-ups. `replayed` sets it.
  offset: number
  acked: number
  // What the user typed during a catch-up, sent after it in order.
  readonly typed: string[]
  // This client took the shell's size (spec 2.1). One that did not sends its size first when it
  // claims, so the daemon never takes its old size from the attach.
  claimed: boolean
  // The visibility the daemon knows.
  sentVisible: boolean
  // The terminal came on screen during a catch-up. It claims once its own size is back.
  claimAfterCatchUp: boolean
  // The size that this screen sent last. It sends a size when it changes, and again when the
  // screen takes the focus: another screen of the same shell may have set its own size since.
  // Never during a catch-up: the local resizes to the snapshot size and back are not the user's.
  sentSize: { cols: number; rows: number }
  // Whether `offset` is right for a resume (spec 5): set by `replayed`, cleared when a catch-up
  // starts.
  offsetKnown: boolean
}
