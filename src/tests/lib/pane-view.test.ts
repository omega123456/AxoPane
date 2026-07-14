import { describe, expect, it } from 'vitest'
import {
  isPaneViewMode,
  paneViewMetadata,
  paneViewModes,
  resolvePaneViewMode,
} from '@/lib/pane-view'

describe('pane-view', () => {
  it('exposes the stable modes and their display metadata', () => {
    expect(paneViewModes).toEqual(['details', 'icons', 'thumbnails'])
    expect(paneViewMetadata).toEqual({
      details: { label: 'Details', icon: 'List' },
      icons: { label: 'Icons', icon: 'Grid2X2' },
      thumbnails: { label: 'Large thumbnails', icon: 'Images' },
    })
  })

  it('validates only stable view modes', () => {
    expect(isPaneViewMode('icons')).toBe(true)
    expect(isPaneViewMode('tiles')).toBe(false)
    expect(isPaneViewMode(null)).toBe(false)
  })

  it('uses the configured fallback for missing or invalid saved values', () => {
    expect(resolvePaneViewMode('thumbnails', 'icons')).toBe('thumbnails')
    expect(resolvePaneViewMode(undefined, 'icons')).toBe('icons')
    expect(resolvePaneViewMode('legacy', 'icons')).toBe('icons')
  })
})
