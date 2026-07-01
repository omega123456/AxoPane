import { create } from 'zustand'
import { log } from '@/lib/app-log-commands'
import {
  checkForAppUpdate,
  downloadAndInstallAppUpdate,
  summarizeUpdate,
  type AppUpdate,
  type UpdateSummary,
} from '@/lib/updater'
import { UPDATE_INTERVAL_MS, type UpdateInterval } from '@/lib/update-intervals'
import { useConfigStore } from '@/stores/config-store'

type UpdaterStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'installing' | 'error'

// How long a manual "you're up to date" acknowledgement lingers before the
// status quietly returns to idle.
const UP_TO_DATE_RESET_MS = 4_000

let periodicTimer: ReturnType<typeof setInterval> | null = null
let upToDateTimer: ReturnType<typeof setTimeout> | null = null

function clearPeriodicTimer() {
  if (periodicTimer !== null) {
    clearInterval(periodicTimer)
    periodicTimer = null
  }
}

function clearUpToDateTimer() {
  if (upToDateTimer !== null) {
    clearTimeout(upToDateTimer)
    upToDateTimer = null
  }
}

function scheduleUpToDateReset() {
  clearUpToDateTimer()
  upToDateTimer = setTimeout(() => {
    upToDateTimer = null
    if (useUpdaterStore.getState().status === 'up-to-date') {
      useUpdaterStore.setState({ status: 'idle' })
    }
  }, UP_TO_DATE_RESET_MS)
}

function settleNoAvailableUpdate(
  set: (partial: Partial<UpdaterStore>) => void,
  acknowledge: boolean,
) {
  clearUpToDateTimer()

  if (acknowledge) {
    set({ update: null, summary: null, status: 'up-to-date', error: null })
    scheduleUpToDateReset()
    return
  }

  set({ update: null, summary: null, status: 'idle', error: null })
}

function toErrorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause)
}

type UpdaterStore = {
  update: AppUpdate | null
  summary: UpdateSummary | null
  status: UpdaterStatus
  error: string | null
  setAvailable: (update: AppUpdate, summary: UpdateSummary) => void
  setStatus: (status: UpdaterStatus, error?: string | null) => void
  dismiss: () => void
  checkForUpdate: (manual: boolean) => Promise<void>
  downloadAndInstall: () => Promise<void>
  startPeriodicCheck: () => void
  stopPeriodicCheck: () => void
  restartPeriodicCheck: () => void
}

export const useUpdaterStore = create<UpdaterStore>((set, get) => ({
  update: null,
  summary: null,
  status: 'idle',
  error: null,

  setAvailable: (update, summary) => set({ update, summary, status: 'available', error: null }),

  setStatus: (status, error = null) => set({ status, error }),

  dismiss: () => {
    clearUpToDateTimer()
    set({ update: null, summary: null, status: 'idle', error: null })
  },

  checkForUpdate: async (manual) => {
    const { status, update: currentUpdate } = get()
    if (status === 'checking' || status === 'installing') {
      return
    }

    clearUpToDateTimer()

    if (manual || !currentUpdate) {
      set({ status: 'checking', error: null })
    }

    try {
      const update = await checkForAppUpdate()

      if (update) {
        set({ update, summary: summarizeUpdate(update), status: 'available', error: null })
        return
      }

      settleNoAvailableUpdate(set, manual)
    } catch (cause) {
      const message = toErrorMessage(cause)
      log.error('app update check failed', { manual, error: message })

      if (manual) {
        set({ update: null, summary: null, status: 'error', error: message })
        return
      }

      settleNoAvailableUpdate(set, false)
    }
  },

  downloadAndInstall: async () => {
    const { update, status } = get()
    if (!update || status === 'checking' || status === 'installing') {
      return
    }

    set({ status: 'installing', error: null })
    try {
      const latestUpdate = await checkForAppUpdate()
      if (!latestUpdate) {
        settleNoAvailableUpdate(set, true)
        return
      }

      set({
        update: latestUpdate,
        summary: summarizeUpdate(latestUpdate),
        status: 'installing',
        error: null,
      })
      await downloadAndInstallAppUpdate(latestUpdate)
      // On Windows the NSIS passive installer relaunches the app, so reaching
      // this point usually means the process is on its way out.
    } catch (cause) {
      const message = toErrorMessage(cause)
      log.error('app update install failed', { error: message })
      set({ status: 'error', error: message })
    }
  },

  startPeriodicCheck: () => {
    clearPeriodicTimer()

    // Always check once on launch, regardless of the configured cadence.
    void get().checkForUpdate(false)

    const interval = useConfigStore.getState().updateCheckInterval as UpdateInterval
    if (interval === 'off') {
      return
    }

    const intervalMs = UPDATE_INTERVAL_MS[interval]
    if (!intervalMs) {
      return
    }

    periodicTimer = setInterval(() => {
      void useUpdaterStore.getState().checkForUpdate(false)
    }, intervalMs)
  },

  stopPeriodicCheck: () => {
    clearPeriodicTimer()
    clearUpToDateTimer()
  },

  restartPeriodicCheck: () => {
    get().startPeriodicCheck()
  },
}))
