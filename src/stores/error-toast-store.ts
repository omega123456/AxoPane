import { create } from 'zustand'
import { openAdminRestartDialog } from '@/stores/action-dialog-store'

const AUTO_DISMISS_MS = 6_000

type ShowOptions = { offerAdminRestart?: boolean }

type ErrorToastStore = {
  message: string | null
  show: (message: string, options?: ShowOptions) => void
  dismiss: () => void
}

let dismissTimer: ReturnType<typeof setTimeout> | undefined

export const useErrorToastStore = create<ErrorToastStore>((set) => ({
  message: null,
  show: (message, options) => {
    const routedToAdminRestart =
      options?.offerAdminRestart !== false && openAdminRestartDialog(message)
    if (routedToAdminRestart) {
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
