// Class names for the shared controls, so every button and field looks and sizes the same.

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'link'
export type ButtonSize = 'xs' | 'sm' | 'md'

// Icon sizes in px. One icon size per job, so rows and toolbars line up.
export const ICON = { xs: 12, sm: 14, md: 16, lg: 20, xl: 32 } as const

const BUTTON_BASE =
  'inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md text-sm font-medium whitespace-nowrap select-none disabled:pointer-events-none disabled:opacity-50 aria-disabled:opacity-50'

const BUTTON_VARIANT: Record<ButtonVariant, string> = {
  primary:
    'bg-ink text-canvas not-aria-disabled:hover:bg-ink/85 not-aria-disabled:active:bg-ink/75',
  secondary:
    'border border-control bg-raised text-ink ring-contrast not-aria-disabled:hover:border-control-hover not-aria-disabled:hover:bg-[color-mix(in_srgb,var(--raised)_92%,var(--ink))] not-aria-disabled:active:bg-[color-mix(in_srgb,var(--raised)_85%,var(--ink))]',
  ghost:
    'text-ink-2 not-aria-disabled:hover:bg-hover not-aria-disabled:hover:text-ink not-aria-disabled:active:bg-active',
  danger: 'bg-danger text-white not-aria-disabled:hover:bg-danger/85',
  link: 'text-link not-aria-disabled:hover:underline',
}

const BUTTON_SIZE: Record<ButtonSize, string> = {
  xs: 'h-(--control-h-xs) px-2',
  sm: 'h-(--control-h) px-2.5',
  md: 'h-(--control-h-md) px-3',
}

export function button(variant: ButtonVariant = 'secondary', size: ButtonSize = 'sm'): string {
  // A link has no box, so it has no size.
  return `${BUTTON_BASE} ${BUTTON_VARIANT[variant]} ${variant === 'link' ? '' : BUTTON_SIZE[size]}`
}

export type IconButtonSize = 'xs' | 'sm'

const ICON_BUTTON_SIZE: Record<IconButtonSize, string> = {
  xs: 'size-(--control-h-xs)',
  sm: 'size-(--control-h)',
}

// A square button that holds only an icon. Use the IconButton component: it needs a name.
export function iconButton(size: IconButtonSize = 'sm'): string {
  return `inline-flex ${ICON_BUTTON_SIZE[size]} shrink-0 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-ink active:bg-active disabled:pointer-events-none disabled:opacity-50 aria-pressed:bg-selected aria-pressed:text-ink`
}

const FIELD =
  'min-w-0 rounded-md border border-control bg-input text-ink ring-contrast placeholder:text-placeholder hover:border-control-hover'

export const input = `h-(--control-h) px-2.5 ${FIELD}`

// Fields in dialogs are 32px.
export const inputLg = `h-(--control-h-md) px-2.5 ${FIELD}`

export const textarea = `px-2.5 py-2 ${FIELD}`

// A box that looks like a field and holds an input and icons. The ring follows the focus inside.
export const fieldBox =
  'flex items-center gap-2 rounded-md border border-control bg-input px-2.5 text-muted ring-contrast hover:border-control-hover focus-within:border-focus focus-within:outline focus-within:outline-1 focus-within:-outline-offset-1 focus-within:outline-focus'

export const kbd =
  'inline-flex h-[1.125rem] min-w-[1.125rem] items-center justify-center rounded border border-line bg-raised px-1 text-2xs font-medium text-muted shadow-[inset_0_-1px_0_var(--line-strong)]'

// A row in a list or a menu: 28px, quiet hover, a ring that stays inside the row.
export const listRow =
  'focus-inset flex h-(--row-h) w-full items-center gap-1.5 rounded-md px-2 text-left text-ink-2 hover:bg-hover hover:text-ink aria-pressed:bg-selected aria-pressed:text-ink'

// A small caps heading over a group of fields or rows.
export const sectionTitle =
  'text-2xs font-semibold uppercase tracking-[0.06em] text-muted'
