import { Dialog } from './ui/Dialog'
import { Kbd } from './ui/Kbd'

// Until the command registry exists (spec section 4.17), this lists the keys that work today.
const GROUPS: readonly { title: string; rows: readonly { keys: string; text: string }[] }[] = [
  {
    title: 'Global',
    rows: [
      { keys: 'Mod+B', text: 'Show or hide the sidebar' },
      { keys: 'Mod+Alt+B', text: 'Show or hide the details' },
      { keys: 'F6', text: 'Move to the next part of the window' },
      { keys: 'Shift+F6', text: 'Move to the previous part' },
      { keys: '?', text: 'Show this list' },
      { keys: 'Escape', text: 'Close the top layer, or go back to All tasks' },
    ],
  },
  {
    title: 'Tasks',
    rows: [
      { keys: '/', text: 'Search the sidebar' },
      { keys: 'c', text: 'New workspace' },
      { keys: 'n', text: 'Add a task' },
      { keys: 'j', text: 'Next task' },
      { keys: 'k', text: 'Previous task' },
      { keys: 'Enter', text: 'Open the details of the selected task' },
    ],
  },
]

export function ShortcutSheet({ onClose }: { onClose: () => void }) {
  return (
    <Dialog title="Keyboard shortcuts" size="md" onClose={onClose}>
      <div className="flex flex-col gap-4 pb-2">
        {GROUPS.map((group) => (
          <section key={group.title} aria-label={group.title} className="flex flex-col gap-1">
            <h3 className="text-2xs font-semibold tracking-[0.06em] text-muted uppercase">
              {group.title}
            </h3>
            <ul className="flex flex-col">
              {group.rows.map((row) => (
                <li
                  key={row.keys}
                  className="flex h-7 items-center justify-between gap-4 border-b border-line last:border-b-0"
                >
                  <span>{row.text}</span>
                  <Kbd shortcut={row.keys} />
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </Dialog>
  )
}
