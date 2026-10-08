import { X } from 'lucide-react'
import { type ReactNode, useEffect, useId, useRef } from 'react'
import { createPortal } from 'react-dom'
import { button, iconButton } from './styles'

type DialogProps = {
  title: string
  description?: ReactNode
  onClose: () => void
  children?: ReactNode
  footer?: ReactNode
  // Wider for forms with several fields.
  size?: 'sm' | 'md'
}

// A modal over the page. Escape and a click outside close it; focus goes to its first field and
// back to where it was when it closes.
export function Dialog({
  title,
  description,
  onClose,
  children,
  footer,
  size = 'sm',
}: DialogProps) {
  const panel = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  const titleId = useId()

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const first = panel.current?.querySelector<HTMLElement>(
      '[data-autofocus], input, textarea, select, button:not([data-dialog-close])',
    )
    first?.focus()
    // Before the dashboard's own keys, so Escape closes only the dialog.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        close.current()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => {
      window.removeEventListener('keydown', onKeyDown, true)
      previous?.focus()
    }
  }, [])

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-start justify-center p-4 pt-[14vh]">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close the dialog"
        data-dialog-close
        onClick={onClose}
        className="absolute inset-0 cursor-default bg-black/55 backdrop-blur-[1px]"
      />
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className={`relative flex max-h-[76vh] w-full flex-col overflow-hidden rounded-xl border border-line-strong bg-surface shadow-2xl shadow-black/40 ${size === 'md' ? 'max-w-lg' : 'max-w-sm'}`}
      >
        <header className="flex items-start gap-3 px-5 pt-4 pb-1">
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="text-[0.9375rem] font-semibold tracking-tight">
              {title}
            </h2>
            {description !== undefined && <p className="mt-1 text-muted">{description}</p>}
          </div>
          <button
            type="button"
            aria-label="Close"
            data-dialog-close
            onClick={onClose}
            className={`${iconButton} -mr-2`}
          >
            <X size={15} aria-hidden="true" />
          </button>
        </header>
        {children !== undefined && <div className="overflow-y-auto px-5 py-3">{children}</div>}
        {footer !== undefined && (
          <footer className="flex justify-end gap-2 border-t border-line bg-raised/50 px-5 py-3">
            {footer}
          </footer>
        )}
      </div>
    </div>,
    document.body,
  )
}

type ConfirmDialogProps = {
  title: string
  description: ReactNode
  confirmLabel: string
  onConfirm: () => void
  onClose: () => void
}

// Asks before something that cannot be undone.
export function ConfirmDialog({
  title,
  description,
  confirmLabel,
  onConfirm,
  onClose,
}: ConfirmDialogProps) {
  return (
    <Dialog
      title={title}
      description={description}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose} className={button('ghost')}>
            Cancel
          </button>
          <button
            type="button"
            data-autofocus
            onClick={() => {
              onClose()
              onConfirm()
            }}
            className={button('danger')}
          >
            {confirmLabel}
          </button>
        </>
      }
    />
  )
}
