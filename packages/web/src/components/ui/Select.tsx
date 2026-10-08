import { ChevronDown } from 'lucide-react'
import type { SelectHTMLAttributes } from 'react'

// A native select, so the keyboard and screen readers work as usual, drawn like the other fields.
export function Select({ className = '', children, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className={`relative inline-flex min-w-0 ${className}`}>
      <select
        {...props}
        className="h-7 w-full min-w-0 appearance-none truncate rounded-md border border-line bg-surface pr-7 pl-2 text-ink hover:border-line-strong"
      >
        {children}
      </select>
      <ChevronDown
        size={13}
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-muted"
      />
    </span>
  )
}
