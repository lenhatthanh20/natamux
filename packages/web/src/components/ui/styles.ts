// Class names for the shared controls, so every button and field looks and sizes the same.

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'
export type ButtonSize = 'sm' | 'md'

const BUTTON_BASE =
  'inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap select-none disabled:pointer-events-none disabled:opacity-50'

const BUTTON_VARIANT: Record<ButtonVariant, string> = {
  primary: 'bg-ink text-canvas hover:bg-ink/85',
  secondary: 'border border-line bg-raised text-ink hover:border-line-strong hover:bg-hover',
  ghost: 'text-muted hover:bg-hover hover:text-ink',
  danger: 'bg-danger text-white hover:bg-danger/85',
}

const BUTTON_SIZE: Record<ButtonSize, string> = {
  sm: 'h-7 px-2.5',
  md: 'h-8 px-3',
}

export function button(variant: ButtonVariant = 'secondary', size: ButtonSize = 'sm'): string {
  return `${BUTTON_BASE} ${BUTTON_VARIANT[variant]} ${BUTTON_SIZE[size]}`
}

// A square button that holds only an icon. It needs an aria-label.
export const iconButton =
  'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-ink disabled:pointer-events-none disabled:opacity-50'

export const input =
  'h-8 min-w-0 rounded-md border border-line bg-surface px-2.5 text-ink placeholder:text-muted/70 hover:border-line-strong'

export const textarea =
  'min-w-0 rounded-md border border-line bg-surface px-2.5 py-2 text-ink placeholder:text-muted/70 hover:border-line-strong'

export const kbd =
  'inline-flex h-[1.125rem] min-w-[1.125rem] items-center justify-center rounded border border-line bg-raised px-1 text-muted'

// A small heading over a group of fields or rows.
export const sectionTitle = 'text-xs font-medium text-muted'
