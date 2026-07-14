import { describe, expect, it } from 'vitest'
import {
  MAX_THUMBNAIL_CACHE_BYTES,
  MAX_THUMBNAIL_CACHE_ENTRIES,
  THUMBNAIL_NEGATIVE_TTL_MS,
  pruneThumbnailCache,
  thumbnailWeight,
} from '@/stores/thumbnails/cache-policy'

describe('thumbnail cache policy', () => {
  it('estimates UTF-16 data-url storage and expires unprotected negative records', () => {
    const now = 100_000
    expect(thumbnailWeight('abc')).toBe(6)
    const cache = {
      expired: { state: 'failed' as const, dataUrl: null, touched: now - THUMBNAIL_NEGATIVE_TTL_MS, weight: 1 },
      visible: { state: 'unavailable' as const, dataUrl: null, touched: now - THUMBNAIL_NEGATIVE_TTL_MS, weight: 1 },
    }
    expect(pruneThumbnailCache(cache, new Set(['visible']), now)).toEqual({ visible: cache.visible })
  })

  it('uses deterministic oldest-first eviction for entry and weight bounds', () => {
    const cache = Object.fromEntries(
      Array.from({ length: MAX_THUMBNAIL_CACHE_ENTRIES + 1 }, (_, index) => [
        `record-${index}`,
        { state: 'ready' as const, dataUrl: 'x', touched: index, weight: MAX_THUMBNAIL_CACHE_BYTES },
      ]),
    )
    const result = pruneThumbnailCache(cache, new Set(), 100)
    expect(Object.keys(result)).toHaveLength(1)
    expect(result[`record-${MAX_THUMBNAIL_CACHE_ENTRIES}`]).toBeDefined()
  })

  it('retains protected visible records while pruning ordinary candidates', () => {
    const cache = {
      visible: { state: 'ready' as const, dataUrl: 'x', touched: 1, weight: MAX_THUMBNAIL_CACHE_BYTES },
      old: { state: 'ready' as const, dataUrl: 'x', touched: 0, weight: MAX_THUMBNAIL_CACHE_BYTES },
    }
    expect(pruneThumbnailCache(cache, new Set(['visible']), 10)).toEqual({ visible: cache.visible })
  })
})
