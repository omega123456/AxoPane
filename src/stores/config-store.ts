import { create } from 'zustand'
import type { ThemePreference } from '@/lib/types/ipc'
import { persistAppConfig } from '@/lib/app-config'

type ConfigStore = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  hydrate: (config: { theme: ThemePreference; showHiddenFiles: boolean; dismissedEverythingBanner: boolean }) => void
  setThemePreference: (theme: ThemePreference) => Promise<void>
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  dismissEverythingBanner: () => Promise<void>
  reset: () => void
}

function defaultState() {
  return {
    theme: 'system' as ThemePreference,
    showHiddenFiles: false,
    dismissedEverythingBanner: false,
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
  dismissEverythingBanner: async () => {
    set({ dismissedEverythingBanner: true })
    await persistAppConfig()
  },
  reset: () => set(defaultState()),
}))
