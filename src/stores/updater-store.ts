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
import { errorMessage } from '@/lib/error-message'

type UpdaterStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'installing' | 'error'

export type UpdaterOutcome = {
  update: AppUpdate | null
  summary: UpdateSummary | null
  status: UpdaterStatus
  error: string | null
}

export const IDLE_OUTCOME: UpdaterOutcome = {
  update: null,
  summary: null,
  status: 'idle',
  error: null,
}

export function availableOutcome(update: AppUpdate, summary: UpdateSummary): UpdaterOutcome {
  return { update, summary, status: 'available', error: null }
}

export function failedCheckOutcome(error: string): UpdaterOutcome {
  return { ...IDLE_OUTCOME, status: 'error', error }
}

export function withUpdaterStatus(
  outcome: UpdaterOutcome,
  status: UpdaterStatus,
  error: string | null = null,
): UpdaterOutcome {
  return { ...outcome, status, error }
}

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
    const { outcome } = useUpdaterStore.getState()
    if (outcome.status === 'up-to-date') {
      useUpdaterStore.setState({ outcome: withUpdaterStatus(outcome, 'idle', outcome.error) })
    }
  }, UP_TO_DATE_RESET_MS)
}

function settleNoAvailableUpdate(set: UpdaterSet, acknowledge: boolean) {
  clearUpToDateTimer()

  if (acknowledge) {
    set({ outcome: withUpdaterStatus(IDLE_OUTCOME, 'up-to-date') })
    scheduleUpToDateReset()
    return
  }

  set({ outcome: IDLE_OUTCOME })
}

type UpdaterStore = {
  outcome: UpdaterOutcome
  setAvailable: (update: AppUpdate, summary: UpdateSummary) => void
  setStatus: (status: UpdaterStatus, error?: string | null) => void
  dismiss: () => void
  checkForUpdate: (manual: boolean) => Promise<void>
  downloadAndInstall: () => Promise<void>
  startPeriodicCheck: () => void
  stopPeriodicCheck: () => void
  restartPeriodicCheck: () => void
}

type UpdaterSet = (partial: Partial<UpdaterStore>) => void
type UpdaterGet = () => UpdaterStore

function isUpdaterBusy(status: UpdaterStatus) {
  return status === 'checking' || status === 'installing'
}

async function checkForUpdate(set: UpdaterSet, get: UpdaterGet, manual: boolean) {
  const { outcome } = get()
  if (isUpdaterBusy(outcome.status)) {
    return
  }

  clearUpToDateTimer()

  const showsChecking = manual || !outcome.update
  if (showsChecking) {
    set({ outcome: withUpdaterStatus(outcome, 'checking') })
  }

  try {
    const update = await checkForAppUpdate()

    if (update) {
      set({ outcome: availableOutcome(update, summarizeUpdate(update)) })
      return
    }

    settleNoAvailableUpdate(set, manual)
  } catch (cause) {
    const message = errorMessage(cause)
    log.error('app update check failed', { manual, error: message })

    if (manual) {
      set({ outcome: failedCheckOutcome(message) })
      return
    }

    settleNoAvailableUpdate(set, false)
  }
}

async function downloadAndInstall(set: UpdaterSet, get: UpdaterGet) {
  const { outcome } = get()
  if (!outcome.update) {
    return
  }
  if (isUpdaterBusy(outcome.status)) {
    return
  }

  set({ outcome: withUpdaterStatus(outcome, 'installing') })
  try {
    const latestUpdate = await checkForAppUpdate()
    if (!latestUpdate) {
      settleNoAvailableUpdate(set, true)
      return
    }

    const latestOutcome = availableOutcome(latestUpdate, summarizeUpdate(latestUpdate))
    set({ outcome: withUpdaterStatus(latestOutcome, 'installing') })
    await downloadAndInstallAppUpdate(latestUpdate)
    // On Windows the NSIS passive installer relaunches the app, so reaching
    // this point usually means the process is on its way out.
  } catch (cause) {
    const message = errorMessage(cause)
    log.error('app update install failed', { error: message })
    set({ outcome: withUpdaterStatus(get().outcome, 'error', message) })
  }
}

export const useUpdaterStore = create<UpdaterStore>((set, get) => ({
  outcome: IDLE_OUTCOME,

  setAvailable: (update, summary) => set({ outcome: availableOutcome(update, summary) }),

  setStatus: (status, error = null) =>
    set({ outcome: withUpdaterStatus(get().outcome, status, error) }),

  dismiss: () => {
    clearUpToDateTimer()
    set({ outcome: IDLE_OUTCOME })
  },

  checkForUpdate: (manual) => checkForUpdate(set, get, manual),

  downloadAndInstall: () => downloadAndInstall(set, get),

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
