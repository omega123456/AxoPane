import { create } from 'zustand'
import type { LogLevel, ThemePreference } from '@/lib/types/ipc'
import { DEFAULT_DATE_FORMAT, type DateFormat } from '@/lib/date-format'
import { DEFAULT_UPDATE_INTERVAL, type UpdateInterval } from '@/lib/update-intervals'
import { persistAppConfig } from '@/lib/app-config'
import { setLogLevel as setBackendLogLevel } from '@/lib/ipc/commands'

type ConfigSnapshot = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  updateCheckInterval: UpdateInterval
  logLevel: LogLevel
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
  relativeDates: boolean
  autoFolderSize: boolean
}

type ConfigStore = ConfigSnapshot & {
  hydrate: (config: ConfigSnapshot) => void
  setThemePreference: (theme: ThemePreference) => Promise<void>
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  setUpdateCheckInterval: (updateCheckInterval: UpdateInterval) => Promise<void>
  setLogLevel: (logLevel: LogLevel) => Promise<void>
  setDateFormat: (dateFormat: DateFormat) => Promise<void>
  setShowTime: (showTime: boolean) => Promise<void>
  setShowSeconds: (showSeconds: boolean) => Promise<void>
  setRelativeDates: (relativeDates: boolean) => Promise<void>
  setAutoFolderSize: (autoFolderSize: boolean) => Promise<void>
  dismissEverythingBanner: () => Promise<void>
  reset: () => void
}

function defaultState(): ConfigSnapshot {
  return {
    theme: 'system',
    showHiddenFiles: false,
    dismissedEverythingBanner: false,
    updateCheckInterval: DEFAULT_UPDATE_INTERVAL,
    logLevel: 'info',
    dateFormat: DEFAULT_DATE_FORMAT,
    showTime: false,
    showSeconds: false,
    relativeDates: false,
    autoFolderSize: true,
  }
}

export const useConfigStore = create<ConfigStore>((set) => ({
  ...defaultState(),
  hydrate: (config) => set(config),
  setThemePreference: async (theme) => {
    set({ theme })
    await persistAppConfig()
  },
  setShowHiddenFiles: async (showHiddenFiles) => {
    set({ showHiddenFiles })
    await persistAppConfig()
  },
  setUpdateCheckInterval: async (updateCheckInterval) => {
    set({ updateCheckInterval })
    await persistAppConfig()
  },
  setDateFormat: async (dateFormat) => {
    set({ dateFormat })
    await persistAppConfig()
  },
  setShowTime: async (showTime) => {
    set({ showTime })
    await persistAppConfig()
  },
  setShowSeconds: async (showSeconds) => {
    set({ showSeconds })
    await persistAppConfig()
  },
  setRelativeDates: async (relativeDates) => {
    set({ relativeDates })
    await persistAppConfig()
  },
  setAutoFolderSize: async (autoFolderSize) => {
    set({ autoFolderSize })
    await persistAppConfig()
  },
  setLogLevel: async (logLevel) => {
    set({ logLevel })
    // Applies to the running backend logger immediately and persists the level
    // into the app config (config.json), so capture survives restarts.
    await setBackendLogLevel(logLevel)
  },
  dismissEverythingBanner: async () => {
    set({ dismissedEverythingBanner: true })
    await persistAppConfig()
  },
  reset: () => set(defaultState()),
}))
