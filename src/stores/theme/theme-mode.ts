import type { ThemePreference } from '@/lib/types/ipc'

export type ThemeMode = 'light' | 'dark'

const OPPOSITE_THEME: Record<ThemeMode, ThemeMode> = { light: 'dark', dark: 'light' }

export function oppositeTheme(theme: ThemeMode): ThemeMode {
  return OPPOSITE_THEME[theme]
}

export function resolveTheme(preference: ThemePreference, systemTheme: () => ThemeMode): ThemeMode {
  return preference === 'system' ? systemTheme() : preference
}
