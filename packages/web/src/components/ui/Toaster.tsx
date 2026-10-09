import { CircleAlert, CircleCheck, Info, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { type Toast, toastDuration, useToasts } from '../../lib/toasts'
import { Button } from './Button'
import { IconButton } from './IconButton'
import { ICON } from './styles'

const TONE = {
  info: { Icon: Info, color: 'text-accent' },
  success: { Icon: CircleCheck, color: 'text-add' },
  error: { Icon: CircleAlert, color: 'text-danger-text' },
} as const

function ToastItem({ toast }: { toast: Toast }) {
  const dismiss = useToasts((state) => state.dismiss)
  const [paused, setPaused] = useState(false)
  // Time left when the pointer or focus pauses the timer, so it resumes where it stopped.
  const left = useRef(toastDuration(toast))

  useEffect(() => {
    if (paused || left.current === null) {
      return
    }
    const started = Date.now()
    const timer = setTimeout(() => dismiss(toast.id), left.current)
    return () => {
      clearTimeout(timer)
      if (left.current !== null) {
        left.current = Math.max(1000, left.current - (Date.now() - started))
      }
    }
  }, [paused, toast.id, dismiss])

  const { Icon, color } = TONE[toast.tone]
  return (
    <li
      role={toast.tone === 'error' ? 'alert' : 'status'}
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      className="hm-pop pointer-events-auto flex w-full items-start gap-2.5 rounded-lg border border-line-strong bg-popover py-3 pr-3 pl-3.5 shadow-pop ring-1 ring-contrast"
    >
      <Icon size={ICON.md} aria-hidden="true" className={`mt-0.5 shrink-0 ${color}`} />
      <div className="min-w-0 flex-1">
        <p className="break-words text-ink">{toast.text}</p>
        {toast.detail !== undefined && (
          <p className="mt-0.5 break-words text-xs text-ink-2">{toast.detail}</p>
        )}
        {toast.actions !== undefined && toast.actions.length > 0 && (
          <div className="mt-1.5 flex gap-3">
            {toast.actions.map((action) => (
              <Button
                key={action.label}
                variant="link"
                onClick={() => {
                  action.run()
                  dismiss(toast.id)
                }}
              >
                {action.label}
              </Button>
            ))}
          </div>
        )}
      </div>
      <IconButton
        label="Dismiss"
        icon={X}
        size="xs"
        onClick={() => dismiss(toast.id)}
        className="-my-1 -mr-1"
      />
    </li>
  )
}

// Short messages after an action, in the corner above the status bar. It sits on the page itself,
// so a dialog's scrim and inert page do not hide it.
export function Toaster() {
  const toasts = useToasts((state) => state.toasts)
  return createPortal(
    <ol className="pointer-events-none fixed right-4 bottom-10 z-(--z-palette) flex w-90 max-w-[calc(100vw-2rem)] flex-col gap-2 max-md:right-2 max-md:left-2 max-md:w-auto max-md:max-w-none">
      {toasts.map((item) => (
        <ToastItem key={item.id} toast={item} />
      ))}
    </ol>,
    document.body,
  )
}
