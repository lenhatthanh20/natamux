import { ChevronRight } from 'lucide-react'
import type { ReactNode } from 'react'
import { useStoredFlag } from '../hooks/useStoredFlag'
import { CountBadge } from './ui/CountBadge'
import { ICON, sectionTitle } from './ui/styles'

type DetailSectionProps = {
  // Names the section for screen readers and keeps its open state in localStorage.
  id: string
  title: string
  defaultOpen?: boolean
  count?: number
  // Sits at the right end of the header, for example a small status text.
  aside?: ReactNode
  children: ReactNode
}

// A part of the details panel that folds. Whether it is open is remembered per section.
export function DetailSection({
  id,
  title,
  defaultOpen = true,
  count,
  aside,
  children,
}: DetailSectionProps) {
  const [open, setOpen] = useStoredFlag(`helmsman.section.${id}`, defaultOpen)

  return (
    <section aria-label={title} className="border-t border-line">
      <div className="flex h-8 items-center gap-2 px-4">
        <h3 className="min-w-0 flex-1">
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setOpen(!open)}
            className="focus-inset flex h-7 w-full items-center gap-1.5 text-left"
          >
            <ChevronRight
              size={ICON.xs}
              aria-hidden="true"
              className={`shrink-0 text-muted transition-transform ${open ? 'rotate-90' : ''}`}
            />
            <span className={sectionTitle}>{title}</span>
            {count !== undefined && <CountBadge count={count} />}
          </button>
        </h3>
        {aside}
      </div>
      {open && <div className="flex flex-col gap-2 px-4 pt-1 pb-4">{children}</div>}
    </section>
  )
}

// A label and its value, in the details panel's two columns.
export function Field({
  label,
  children,
  action,
}: {
  label: string
  children: ReactNode
  // A small icon button that shows when the row is hovered or focused, like Copy.
  action?: ReactNode
}) {
  return (
    <div className="group grid min-h-6 grid-cols-[6rem_minmax(0,1fr)] items-start gap-2">
      <dt className="py-0.5 text-xs text-muted">{label}</dt>
      <dd className="flex min-w-0 items-start gap-1 break-words">
        <span className="min-w-0 flex-1 py-px">{children}</span>
        {action !== undefined && (
          <span className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100">
            {action}
          </span>
        )}
      </dd>
    </div>
  )
}
