import { CircleAlert, CircleCheck, X } from 'lucide-react'
import { useEffect } from 'react'
import { type Toast, useToasts } from '../../lib/toasts'
import { iconButton } from './styles'

const SHOW_MS = 5000

function ToastItem({ toast }: { toast: Toast }) {
  const dismiss = useToasts((state) => state.dismiss)

  useEffect(() => {
    const timer = setTimeout(() => dismiss(toast.id), toast.tone === 'error' ? SHOW_MS * 2 : SHOW_MS)
    return () => clearTimeout(timer)
  }, [toast, dismiss])

  const Icon = toast.tone === 'error' ? CircleAlert : CircleCheck
  return (
    <li
      role={toast.tone === 'error' ? 'alert' : 'status'}
      className="pointer-events-auto flex w-80 items-start gap-2.5 rounded-lg border border-line-strong bg-raised py-2.5 pr-2 pl-3 shadow-xl shadow-black/30"
    >
      <Icon
        size={15}
        aria-hidden="true"
        className={`mt-0.5 shrink-0 ${toast.tone === 'error' ? 'text-remove' : 'text-add'}`}
      />
      <p className="min-w-0 flex-1 break-words">{toast.text}</p>
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => dismiss(toast.id)}
        className={`${iconButton} -my-1 size-6`}
      >
        <X size={13} aria-hidden="true" />
      </button>
    </li>
  )
}

// Short messages after an action, in the corner above the status bar.
export function Toaster() {
  const toasts = useToasts((state) => state.toasts)
  return (
    <ol className="pointer-events-none fixed right-4 bottom-10 z-40 flex flex-col items-end gap-2">
      {toasts.map((item) => (
        <ToastItem key={item.id} toast={item} />
      ))}
    </ol>
  )
}
