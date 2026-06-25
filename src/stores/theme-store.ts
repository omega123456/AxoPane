import { create } from 'zustand'
import type { ThemePreference } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'

export type ThemeMode = 'light' | 'dark'

type ThemeStore = {
  preference: ThemePreference
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
  setThemePreference: (preference: ThemePreference) => void
  toggleTheme: () => void
}

const ROOT_CLASS = 'dark'
const SYSTEM_MEDIA_QUERY = '(prefers-color-scheme: dark)'

function systemTheme(): ThemeMode {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return 'dark'
  }

  return window.matchMedia(SYSTEM_MEDIA_QUERY).matches ? 'dark' : 'light'
}

function resolveTheme(preference: ThemePreference): ThemeMode {
  return preference === 'system' ? systemTheme() : preference
}

function applyTheme(theme: ThemeMode) {
  document.documentElement.classList.toggle(ROOT_CLASS, theme === 'dark')
}

function syncTheme(preference: ThemePreference) {
  const theme = resolveTheme(preference)
  applyTheme(theme)
  return { preference, theme }
}

export const useThemeStore = create<ThemeStore>((set) => ({
  ...syncTheme(useConfigStore.getState().theme),
  setTheme: (theme) => {
    set(syncTheme(theme))
  },
  setThemePreference: (preference) => {
    set(syncTheme(preference))
  },
  toggleTheme: () =>
    set((state) => {
      const nextTheme: ThemeMode = state.theme === 'dark' ? 'light' : 'dark'
      return syncTheme(nextTheme)
    }),
}))

export function initializeTheme() {
  useThemeStore.getState().setThemePreference(useConfigStore.getState().theme)
}
