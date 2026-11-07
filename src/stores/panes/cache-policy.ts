import { millisecondsInMinute } from 'date-fns/constants'
import { BYTES_PER_MEBIBYTE } from '@/lib/numbers'
import { pruneWeightedCache, type WeightedCacheRecord } from '@/stores/weighted-cache'

/** Deterministic bounded metadata cache policies used by pane state. */
export const MAX_SIZE_CACHE_ENTRIES = 10_000
export const MAX_ICON_CACHE_ENTRIES = 4_096
const MAX_ICON_CACHE_MEBIBYTES = 32
export const MAX_ICON_CACHE_BYTES = MAX_ICON_CACHE_MEBIBYTES * BYTES_PER_MEBIBYTE
const ICON_NEGATIVE_TTL_MINUTES = 10
export const ICON_NEGATIVE_TTL_MS = ICON_NEGATIVE_TTL_MINUTES * millisecondsInMinute

export type IconCacheRecord = WeightedCacheRecord & { value: string | null }

/** Negative records expire first, then the oldest unprotected records go. */
export function pruneIconCache(
  cache: Record<string, IconCacheRecord>,
  protectedPaths: ReadonlySet<string>,
  now: number,
  nativeIconsSupported: boolean,
): Record<string, IconCacheRecord> {
  const isExpired = (record: IconCacheRecord) =>
    record.value === null && (!nativeIconsSupported || now - record.touched >= ICON_NEGATIVE_TTL_MS)
  return pruneWeightedCache(cache, protectedPaths, isExpired, {
    maxEntries: MAX_ICON_CACHE_ENTRIES,
    maxBytes: MAX_ICON_CACHE_BYTES,
  })
}

export function pruneSizeCache<T>(
  cache: Record<string, T>,
  protectedPaths: ReadonlySet<string>,
): Record<string, T> {
  const keys = Object.keys(cache)
  if (keys.length <= MAX_SIZE_CACHE_ENTRIES) return cache
  const next = { ...cache }
  for (const key of keys.sort()) {
    if (Object.keys(next).length <= MAX_SIZE_CACHE_ENTRIES) break
    if (!protectedPaths.has(key)) delete next[key]
  }
  return next
}
