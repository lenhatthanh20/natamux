import { CLOSE_GONE, CLOSE_REFUSED, CLOSE_UNAUTHORIZED } from '@natamux/core'

// A burst of resizes (dragging a divider) fits the terminal and sends its size once, when it
// settles.
export const FIT_DELAY_MS = 100
// As VS Code's terminalResizeDebouncer: rows change at once, they cost little. Columns reflow every
// wrapped line, so a buffer of this many rows or more changes them only once the pane settles.
export const REFLOW_WAIT_ROWS = 200
// A pane that changes size hides its screen until the shell drew for the new size: shown once the
// shell printed nothing for REDRAW_QUIET_MS after the resize, or REDRAW_MAX_MS after it at most.
export const REDRAW_QUIET_MS = 60
export const REDRAW_MAX_MS = 400
export const DISPOSE_DELAY_MS = 200
// A catch-up whose `replayed` never comes ends after this, so a daemon bug never leaves a blank
// pane (spec 6).
export const CATCH_UP_TIMEOUT_MS = 10_000
// A resume up to this size is drawn in place. A larger one stays hidden until it ends, as a
// snapshot does.
export const HIDDEN_RESUME_BYTES = 64 * 1024
// After these, the shell is gone or the daemon said no: a new socket would not help.
export const FINAL_CLOSE_CODES = [CLOSE_UNAUTHORIZED, CLOSE_REFUSED, CLOSE_GONE]
