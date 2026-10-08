import { create } from 'zustand'

export type ToastTone = 'info' | 'success' | 'error'

export type ToastAction = { label: string; run: () => void }

export type Toast = {
  id: number
  text: string
  tone: ToastTone
  detail?: string
  actions?: readonly ToastAction[]
}

type ToastStore = {
  toasts: Toast[]
  dismiss: (id: number) => void
}

const MAX_TOASTS = 3

let nextId = 1

export const useToasts = create<ToastStore>()((set) => ({
  toasts: [],
  dismiss: (id) => set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),
}))

// How long a toast stays, in ms. Null: until the user dismisses it.
export function toastDuration(toast: Pick<Toast, 'tone' | 'actions'>): number | null {
  if (toast.tone === 'error') {
    return toast.actions !== undefined && toast.actions.length > 0 ? null : 10_000
  }
  return 5000
}

// The same message twice in a row shows once.
export function toast(
  text: string,
  tone: ToastTone = 'info',
  extra: { detail?: string; actions?: readonly ToastAction[] } = {},
): void {
  useToasts.setState((state) => ({
    toasts: [
      ...state.toasts.filter((item) => item.text !== text),
      { id: nextId++, text, tone, ...extra },
    ].slice(-MAX_TOASTS),
  }))
}
