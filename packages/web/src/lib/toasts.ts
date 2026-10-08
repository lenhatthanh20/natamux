import { create } from 'zustand'

export type ToastTone = 'info' | 'error'

export type Toast = { id: number; text: string; tone: ToastTone }

type ToastStore = {
  toasts: Toast[]
  dismiss: (id: number) => void
}

let nextId = 1

export const useToasts = create<ToastStore>()((set) => ({
  toasts: [],
  dismiss: (id) => set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),
}))

// The same message twice in a row shows once.
export function toast(text: string, tone: ToastTone = 'info'): void {
  useToasts.setState((state) => ({
    toasts: [
      ...state.toasts.filter((item) => item.text !== text),
      { id: nextId++, text, tone },
    ].slice(-4),
  }))
}
