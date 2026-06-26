import { create } from 'zustand'
import type { ThemePreference } from '@/lib/types/ipc'
import { DEFAULT_UPDATE_INTERVAL, type UpdateInterval } from '@/lib/update-intervals'
import { persistAppConfig } from '@/lib/app-config'

type ConfigSnapshot = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  updateCheckInterval: UpdateInterval
}

type ConfigStore = ConfigSnapshot & {
  hydrate: (config: ConfigSnapshot) => void
  setThemePreference: (theme: ThemePreference) => Promise<void>
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  setUpdateCheckInterval: (updateCheckInterval: UpdateInterval) => Promise<void>
  dismissEverythingBanner: () => Promise<void>
  reset: () => void
}

function defaultState(): ConfigSnapshot {
  return {
    theme: 'system',
    showHiddenFiles: false,
    dismissedEverythingBanner: false,
    updateCheckInterval: DEFAULT_UPDATE_INTERVAL,
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
  dismissEverythingBanner: async () => {
    set({ dismissedEverythingBanner: true })
    await persistAppConfig()
  },
  reset: () => set(defaultState()),
}))
