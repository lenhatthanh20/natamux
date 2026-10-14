import type { TaskState } from '@natamux/core'
import { ShapeGlyph } from './ShapeGlyph'

// The state of a task in the table, the palette, pane tabs and the terminal header: the sidebar
// shape at 10 px in a 16 px box, so the two read as one set. A running agent blinks; a quiet one
// shows the "needs you" half disk instead. Reduced motion is CSS only (styles.css).
// `tip` adds to the tooltip, such as the state of each stage of a group.
export function StatusGlyph({
  state,
  quiet = false,
  tip,
}: {
  state: TaskState
  quiet?: boolean
  tip?: string
}) {
  return <ShapeGlyph state={state} quiet={quiet} tip={tip} className="size-4" />
}
