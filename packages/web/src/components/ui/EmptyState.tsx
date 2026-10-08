import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { ICON } from './styles'

type EmptyStateProps = {
  icon: LucideIcon
  title: string
  children?: ReactNode
  // The one primary action, and optionally a link beside it.
  actions?: ReactNode
  // For an error: the icon takes the danger color.
  tone?: 'neutral' | 'error'
}

export function EmptyState({ icon: Icon, title, children, actions, tone = 'neutral' }: EmptyStateProps) {
  return (
    <div className="mx-auto flex max-w-90 flex-col items-center gap-2 px-4 py-10 text-center">
      <span className="mb-1 flex size-14 items-center justify-center rounded-full bg-raised">
        <Icon
          size={ICON.xl}
          strokeWidth={1.5}
          aria-hidden="true"
          className={tone === 'error' ? 'text-danger-text' : 'text-muted'}
        />
      </span>
      <h2 className="text-md font-semibold tracking-tight">{title}</h2>
      {children !== undefined && <div className="text-muted">{children}</div>}
      {actions !== undefined && <div className="mt-2 flex items-center gap-3">{actions}</div>}
    </div>
  )
}
