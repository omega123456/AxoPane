import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { usePanesStore } from '@/stores/panes-store'
import { applyIconStates } from '@/stores/panes/icon-actions'

const originalPlatform = navigator.platform
const UNLISTED_PATH = '/unlisted/app.exe'
const MISSED_ICON = { path: UNLISTED_PATH, iconDataUrl: null }

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

function cachedMissForUnlistedPath() {
  return usePanesStore.getState().resolvedIconPaths[UNLISTED_PATH]
}

describe('applyIconStates', () => {
  beforeEach(() => {
    usePanesStore.getState().reset()
  })

  afterEach(() => {
    setPlatform(originalPlatform)
  })

  it('keeps a missed icon cached on Windows, where native icons can still resolve', () => {
    setPlatform('Win32')

    applyIconStates([MISSED_ICON])

    expect(cachedMissForUnlistedPath()).toBeNull()
  })

  it('drops a missed icon right away on macOS', () => {
    setPlatform('MacIntel')

    applyIconStates([MISSED_ICON])

    expect(cachedMissForUnlistedPath()).toBeUndefined()
  })

  it('follows the shared platform detection for platforms that are not macOS', () => {
    setPlatform('Linux x86_64')

    applyIconStates([MISSED_ICON])

    expect(cachedMissForUnlistedPath()).toBeNull()
  })
})
