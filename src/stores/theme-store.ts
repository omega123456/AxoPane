import { create } from 'zustand'
import type { ThemePreference } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { syncNativeTheme } from '@/lib/window-commands'

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

/** Applies the theme to the document and the native title bar. */
function commitTheme(preference: ThemePreference) {
  void syncNativeTheme(preference)
  return syncTheme(preference)
}

export const useThemeStore = create<ThemeStore>((set) => ({
  // The initial state only touches the DOM; `initializeTheme` syncs the title
  // bar once IPC is available.
  ...syncTheme(useConfigStore.getState().theme),
  setTheme: (theme) => {
    set(commitTheme(theme))
  },
  setThemePreference: (preference) => {
    set(commitTheme(preference))
  },
  toggleTheme: () => set((state) => commitTheme(state.theme === 'dark' ? 'light' : 'dark')),
}))

let systemListenerInstalled = false

/** Re-resolves `system` when the OS switches between light and dark. */
function followSystemTheme() {
  if (systemListenerInstalled || typeof window.matchMedia !== 'function') {
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
