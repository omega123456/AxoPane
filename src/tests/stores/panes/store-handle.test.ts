import { describe, expect, it, vi } from 'vitest'

describe('panes store handle', () => {
  it('refuses access before panes-store binds the store', async () => {
    vi.resetModules()
    const { panesStore } = await import('@/stores/panes/store-handle')

    expect(() => panesStore()).toThrow('panes store used before panes-store created it')
  })

  it('hands back the store panes-store created', async () => {
    vi.resetModules()
    const { usePanesStore } = await import('@/stores/panes-store')
    const { panesStore } = await import('@/stores/panes/store-handle')

    expect(panesStore()).toBe(usePanesStore)
  })
})
