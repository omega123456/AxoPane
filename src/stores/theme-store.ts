import { create } from 'zustand'

export type ThemeMode = 'light' | 'dark'

type ThemeStore = {
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
  toggleTheme: () => void
}

const ROOT_CLASS = 'dark'

function applyTheme(theme: ThemeMode) {
  document.documentElement.classList.toggle(ROOT_CLASS, theme === 'dark')
}

export const useThemeStore = create<ThemeStore>((set) => ({
  theme: 'dark',
  setTheme: (theme) => {
    applyTheme(theme)
    set({ theme })
  },
  toggleTheme: () =>
    set((state) => {
      const nextTheme: ThemeMode = state.theme === 'dark' ? 'light' : 'dark'
      applyTheme(nextTheme)
      return { theme: nextTheme }
    }),
}))

export function initializeTheme() {
  applyTheme(useThemeStore.getState().theme)
}
