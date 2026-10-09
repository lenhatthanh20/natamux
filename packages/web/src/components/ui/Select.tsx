import { ChevronDown } from 'lucide-react'
import type { SelectHTMLAttributes } from 'react'
import { ICON } from './styles'

type SelectProps = Omit<SelectHTMLAttributes<HTMLSelectElement>, 'size'> & {
  // 28px in toolbars, 32px in dialogs.
  fieldSize?: 'sm' | 'md'
}

// A native select, so the keyboard and screen readers work as usual, drawn like the other fields.
export function Select({ className = '', children, fieldSize = 'sm', ...props }: SelectProps) {
  return (
    <span className={`relative inline-flex min-w-0 ${className}`}>
      <select
        {...props}
        className={`${fieldSize === 'md' ? 'h-8' : 'h-7'} w-full min-w-0 appearance-none truncate rounded-md border border-control bg-input pr-7 pl-2.5 text-ink ring-contrast hover:border-control-hover`}
      >
        {children}
      </select>
      <ChevronDown
        size={ICON.sm}
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-muted"
      />
    </span>
  )
}
