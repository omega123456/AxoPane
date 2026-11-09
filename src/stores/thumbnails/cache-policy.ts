import { millisecondsInMinute, millisecondsInSecond } from 'date-fns/constants'
import { BYTES_PER_MEBIBYTE } from '@/lib/numbers'
import { pruneWeightedCache, type WeightedCacheRecord } from '@/stores/weighted-cache'

export const MAX_THUMBNAIL_CACHE_ENTRIES = 256
const MAX_THUMBNAIL_CACHE_MEBIBYTES = 32
export const MAX_THUMBNAIL_CACHE_BYTES = MAX_THUMBNAIL_CACHE_MEBIBYTES * BYTES_PER_MEBIBYTE
const THUMBNAIL_NEGATIVE_TTL_MINUTES = 5
export const THUMBNAIL_NEGATIVE_TTL_MS = THUMBNAIL_NEGATIVE_TTL_MINUTES * millisecondsInMinute
const THUMBNAIL_FAILED_TTL_SECONDS = 30
export const THUMBNAIL_FAILED_TTL_MS = THUMBNAIL_FAILED_TTL_SECONDS * millisecondsInSecond

export type ThumbnailCacheState = 'ready' | 'unavailable' | 'failed'

export type ThumbnailCacheRecord = WeightedCacheRecord & {
  state: ThumbnailCacheState
  quality: 'low' | 'high' | null
  dataUrl: string | null
}

export function isThumbnailCacheRecordUsable(record: ThumbnailCacheRecord, now: number): boolean {
  if (record.state === 'ready') return true
  const ttl = record.state === 'failed' ? THUMBNAIL_FAILED_TTL_MS : THUMBNAIL_NEGATIVE_TTL_MS
  return now - record.touched < ttl
}

/** Expires negatives first, then evicts oldest-first, protected records last. */
export function pruneThumbnailCache(
  cache: Record<string, ThumbnailCacheRecord>,
  protectedKeys: ReadonlySet<string>,
  now: number,
): Record<string, ThumbnailCacheRecord> {
  const isExpired = (record: ThumbnailCacheRecord) => !isThumbnailCacheRecordUsable(record, now)
  const limits = { maxEntries: MAX_THUMBNAIL_CACHE_ENTRIES, maxBytes: MAX_THUMBNAIL_CACHE_BYTES }
  return pruneWeightedCache(cache, protectedKeys, { isExpired, limits, evictProtected: true })
}
