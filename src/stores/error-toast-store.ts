import { create } from 'zustand'

const AUTO_DISMISS_MS = 6_000

type ErrorToastStore = {
  message: string | null
  show: (message: string) => void
  dismiss: () => void
}

let dismissTimer: ReturnType<typeof setTimeout> | undefined

export const useErrorToastStore = create<ErrorToastStore>((set) => ({
  message: null,
  show: (message) => {
    clearTimeout(dismissTimer)
    set({ message })
    dismissTimer = setTimeout(() => set({ message: null }), AUTO_DISMISS_MS)
  },
  dismiss: () => {
    clearTimeout(dismissTimer)
    set({ message: null })
  },
}))
