import { act, waitFor } from '@testing-library/react'
import { vi } from 'vitest'
import { ipc } from '../ipc-mock'
import { useConfigStore } from '@/stores/config-store'
import { initializeTheme, useThemeStore } from '@/stores/theme-store'
import type { SetWindowThemeRequest } from '@/lib/types/ipc'

function systemMedia(initiallyDark: boolean) {
  const listeners = new Set<() => void>()
  const media = {
    matches: initiallyDark,
    addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
  }
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    writable: true,
    value: () => media,
  })
  return {
    switchTo(dark: boolean) {
      media.matches = dark
      listeners.forEach((listener) => listener())
    },
    listenerCount: () => listeners.size,
  }
}

describe('theme-store native title bar', () => {
  it('sends the explicit theme, or null for system, to the native title bar', async () => {
    const themes: SetWindowThemeRequest['theme'][] = []
    const setWindowTheme = vi.fn((payload: SetWindowThemeRequest) => {
      themes.push(payload.theme)
    })
    ipc.override('set_window_theme', setWindowTheme)

    act(() => useThemeStore.getState().setThemePreference('dark'))
    act(() => useThemeStore.getState().toggleTheme())
    act(() => useThemeStore.getState().setThemePreference('system'))

    await waitFor(() => expect(setWindowTheme).toHaveBeenCalledTimes(3))
    expect(themes).toEqual(['dark', 'light', null])
  })

  it('follows OS theme changes while the preference is system, installing one listener', () => {
    const media = systemMedia(false)
    useConfigStore.setState({ theme: 'system' })

    initializeTheme()
    initializeTheme()
    expect(media.listenerCount()).toBe(1)
    expect(useThemeStore.getState().theme).toBe('light')

    act(() => media.switchTo(true))
    expect(useThemeStore.getState().theme).toBe('dark')
    expect(document.documentElement).toHaveClass('dark')

    // An explicit preference ignores the OS.
    act(() => useThemeStore.getState().setThemePreference('light'))
    act(() => media.switchTo(true))
    expect(useThemeStore.getState().theme).toBe('light')
  })
})
