import { describe, expect, it } from 'vitest'
import { oppositeTheme, resolveTheme } from '@/stores/theme/theme-mode'

describe('theme-mode', () => {
  it('flips between light and dark', () => {
    expect(oppositeTheme('light')).toBe('dark')
    expect(oppositeTheme('dark')).toBe('light')
  })

  it('resolves the system preference only when asked to follow it', () => {
    const systemDark = () => 'dark' as const
    expect(resolveTheme('system', systemDark)).toBe('dark')
    expect(resolveTheme('light', systemDark)).toBe('light')
  })
})
