import { create } from 'zustand'
import type { ThemePreference } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { syncNativeTheme } from '@/lib/window-commands'
import { oppositeTheme, resolveTheme, type ThemeMode } from '@/stores/theme/theme-mode'

export type { ThemeMode }

type ThemeStore = {
  preference: ThemePreference
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
  setThemePreference: (preference: ThemePreference) => void
  toggleTheme: () => void
}

const ROOT_CLASS = 'dark'
const SYSTEM_MEDIA_QUERY = '(prefers-color-scheme: dark)'

function canQueryMedia() {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
}

function systemTheme(): ThemeMode {
  if (!canQueryMedia()) {
    return 'dark'
  }

  return window.matchMedia(SYSTEM_MEDIA_QUERY).matches ? 'dark' : 'light'
}

function applyTheme(theme: ThemeMode) {
  document.documentElement.classList.toggle(ROOT_CLASS, theme === 'dark')
}

function syncTheme(preference: ThemePreference) {
  const theme = resolveTheme(preference, systemTheme)
  applyTheme(theme)
  return { preference, theme }
}

function commitTheme(preference: ThemePreference) {
  void syncNativeTheme(preference)
  return syncTheme(preference)
}

export const useThemeStore = create<ThemeStore>((set, get) => ({
  ...syncTheme(useConfigStore.getState().theme),
  setTheme: (theme) => {
    set(commitTheme(theme))
  },
  setThemePreference: (preference) => {
    set(commitTheme(preference))
  },
  toggleTheme: () => set(commitTheme(oppositeTheme(get().theme))),
}))

let systemListenerInstalled = false

function followSystemTheme() {
  if (systemListenerInstalled) {
    return
  }
  if (!canQueryMedia()) {
    return
  }
  systemListenerInstalled = true
  window.matchMedia(SYSTEM_MEDIA_QUERY).addEventListener('change', () => {
    const { preference } = useThemeStore.getState()
    if (preference === 'system') {
      useThemeStore.setState(syncTheme(preference))
    }
  })
}

export function initializeTheme() {
  followSystemTheme()
  useThemeStore.getState().setThemePreference(useConfigStore.getState().theme)
}
