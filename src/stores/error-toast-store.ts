import { create } from 'zustand'
import { openAdminRestartDialog } from '@/stores/action-dialog-store'

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
    // A permission refusal needs an action, not a toast that fades in 6 s.
    if (openAdminRestartDialog(message)) {
      return
    }
    clearTimeout(dismissTimer)
    set({ message })
    dismissTimer = setTimeout(() => set({ message: null }), AUTO_DISMISS_MS)
  },
  dismiss: () => {
    clearTimeout(dismissTimer)
    set({ message: null })
  },
}))
