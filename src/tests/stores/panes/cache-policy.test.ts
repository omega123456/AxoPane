import { describe, expect, it } from 'vitest'
import {
  ICON_NEGATIVE_TTL_MS,
  MAX_ICON_CACHE_BYTES,
  MAX_ICON_CACHE_ENTRIES,
  MAX_SIZE_CACHE_ENTRIES,
  pruneIconCache,
  pruneSizeCache,
} from '@/stores/panes/cache-policy'

describe('pane cache policies', () => {
  it('expires unprotected negative icons while retaining protected paths', () => {
    const now = 100_000
    const cache = {
      old: { value: null, touched: now - ICON_NEGATIVE_TTL_MS, weight: 1 },
      visible: { value: null, touched: now - ICON_NEGATIVE_TTL_MS, weight: 1 },
    }
    expect(pruneIconCache(cache, new Set(['visible']), now, true)).toEqual({
      visible: cache.visible,
    })
  })

  it('does not retain unsupported-platform negative icon history', () => {
    expect(
      pruneIconCache({ none: { value: null, touched: 1, weight: 1 } }, new Set(), 2, false),
    ).toEqual({})
  })

  it('uses real recency so a fresh negative entry is retained until its TTL expires', () => {
    const now = Date.now()
    const cache = { fresh: { value: null, touched: now, weight: 1 } }
    expect(pruneIconCache(cache, new Set(), now, true)).toEqual(cache)
    expect(pruneIconCache(cache, new Set(), now + ICON_NEGATIVE_TTL_MS, true)).toEqual({})
  })

  it('honors icon entry and byte limits deterministically', () => {
    const oversized = Object.fromEntries(
      Array.from({ length: MAX_ICON_CACHE_ENTRIES + 1 }, (_, index) => [
        `icon-${index}`,
        { value: 'x', touched: index, weight: MAX_ICON_CACHE_BYTES },
      ]),
    )
    const result = pruneIconCache(oversized, new Set(), 10, true)
    expect(Object.keys(result)).toHaveLength(1)
    expect(result[`icon-${MAX_ICON_CACHE_ENTRIES}`]).toBeDefined()
  })

  it('bounds size state while preserving selected targets', () => {
    const cache = Object.fromEntries(
      Array.from({ length: MAX_SIZE_CACHE_ENTRIES + 2 }, (_, index) => [`${index}`, index]),
    )
    const result = pruneSizeCache(cache, new Set(['0']))
    expect(Object.keys(result)).toHaveLength(MAX_SIZE_CACHE_ENTRIES)
    expect(result['0']).toBe(0)
  })
})
