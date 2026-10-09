import { X } from 'lucide-react'
import {
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useRef,
} from 'react'
import { createPortal } from 'react-dom'
import { Button } from './Button'
import { IconButton } from './IconButton'

type DialogProps = {
  title: string
  description?: ReactNode
  onClose: () => void
  children?: ReactNode
  footer?: ReactNode
  // 400, 520 and 640px. Wider for forms with several fields.
  size?: 'sm' | 'md' | 'lg'
  // Runs on Mod+Enter from any field. Without it, Mod+Enter presses the primary button of the footer.
  onSubmit?: () => void
  // While busy, Escape and a click outside do not close the dialog.
  busy?: boolean
}

const WIDTH = { sm: 'max-w-100', md: 'max-w-130', lg: 'max-w-160' } as const
const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

// How many dialogs are open, so #root stays inert until the last one closes.
let openDialogs = 0

function setPageInert(inert: boolean) {
  const root = document.getElementById('root')
  if (root === null) {
    return
  }
  if (inert) {
    root.setAttribute('inert', '')
  } else {
    root.removeAttribute('inert')
  }
}

// Makes the page behind a layer inert, until every layer that asked for it has let go. Returns
// the call that lets go.
export function lockPage(): () => void {
  openDialogs += 1
  setPageInert(true)
  return () => {
    openDialogs -= 1
    if (openDialogs === 0) {
      setPageInert(false)
    }
  }
}

// A modal over the page. Escape and a click outside close it; Tab stays inside; the page behind is
// inert; focus goes to its first field and back to where it was when it closes.
export function Dialog({
  title,
  description,
  onClose,
  children,
  footer,
  size = 'sm',
  onSubmit,
  busy = false,
}: DialogProps) {
  const panel = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  const submit = useRef(onSubmit)
  submit.current = onSubmit
  const isBusy = useRef(busy)
  isBusy.current = busy
  const titleId = useId()

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const first = panel.current?.querySelector<HTMLElement>(
      '[data-autofocus], input, textarea, select, button:not([data-dialog-close])',
    )
    first?.focus()
    const unlock = lockPage()
    return () => {
      unlock()
      previous?.focus()
    }
  }, [])

  // Before the dashboard's own keys, so Escape closes only the dialog.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        if (!isBusy.current) {
          close.current()
        }
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [])

  const onPanelKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      if (submit.current !== undefined) {
        submit.current()
      } else {
        panel.current
          ?.querySelector<HTMLElement>(
            'footer [data-variant="primary"]:not([disabled]):not([aria-disabled="true"])',
          )
          ?.click()
      }
      return
    }
    if (event.key !== 'Tab' || panel.current === null) {
      return
    }
    const stops = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
      (element) => element.offsetParent !== null || element === document.activeElement,
    )
    const firstStop = stops[0]
    const lastStop = stops[stops.length - 1]
    if (firstStop === undefined || lastStop === undefined) {
      event.preventDefault()
      return
    }
    if (event.shiftKey && document.activeElement === firstStop) {
      event.preventDefault()
      lastStop.focus()
    } else if (!event.shiftKey && document.activeElement === lastStop) {
      event.preventDefault()
      firstStop.focus()
    }
  }

  return createPortal(
    <div className="fixed inset-0 z-(--z-dialog) flex items-start justify-center p-4 pt-[14vh] max-md:items-end max-md:p-0">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close the dialog"
        data-dialog-close
        onClick={() => {
          if (!busy) {
            onClose()
          }
        }}
        className="hm-scrim absolute inset-0 cursor-default bg-scrim"
      />
      {/* The key handler lets the dialog trap Tab; the panel is not a control itself. */}
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={onPanelKeyDown}
        className={`hm-dialog relative flex max-h-[80vh] w-full flex-col overflow-hidden rounded-xl border border-line-strong bg-popover shadow-modal ring-1 ring-contrast max-md:max-h-[90vh] max-md:max-w-none max-md:rounded-b-none ${WIDTH[size]}`}
      >
        <header className="flex items-start gap-3 px-5 pt-5 pb-2">
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="text-md font-semibold tracking-tight">
              {title}
            </h2>
            {description !== undefined && <p className="mt-1 text-muted">{description}</p>}
          </div>
          <IconButton
            label="Close"
            icon={X}
            data-dialog-close
            onClick={onClose}
            className="-mt-1 -mr-2"
          />
        </header>
        {children !== undefined && <div className="overflow-y-auto px-5 py-3">{children}</div>}
        {footer !== undefined && (
          <footer className="flex justify-end gap-2 border-t border-line px-5 py-3">
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

// Asks before something that cannot be undone. Cancel has the focus, so Enter is the safe answer.
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
          <Button variant="ghost" data-autofocus onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              onClose()
              onConfirm()
            }}
          >
            {confirmLabel}
          </Button>
        </>
      }
    />
  )
}
