import { millisecondsInMinute, millisecondsInSecond } from 'date-fns/constants'
import { BYTES_PER_MEBIBYTE, utf16ByteWeight } from '@/lib/numbers'

export const MAX_THUMBNAIL_CACHE_ENTRIES = 256
const MAX_THUMBNAIL_CACHE_MEBIBYTES = 32
export const MAX_THUMBNAIL_CACHE_BYTES = MAX_THUMBNAIL_CACHE_MEBIBYTES * BYTES_PER_MEBIBYTE
const THUMBNAIL_NEGATIVE_TTL_MINUTES = 5
export const THUMBNAIL_NEGATIVE_TTL_MS = THUMBNAIL_NEGATIVE_TTL_MINUTES * millisecondsInMinute
const THUMBNAIL_FAILED_TTL_SECONDS = 30
export const THUMBNAIL_FAILED_TTL_MS = THUMBNAIL_FAILED_TTL_SECONDS * millisecondsInSecond

export type ThumbnailCacheState = 'ready' | 'unavailable' | 'failed'

export type ThumbnailCacheRecord = {
  state: ThumbnailCacheState
  quality: 'low' | 'high' | null
  dataUrl: string | null
  touched: number
  weight: number
}

export function thumbnailWeight(dataUrl: string | null): number {
  return utf16ByteWeight(dataUrl)
}

export function thumbnailCacheNow(): number {
  return Date.now()
}

export function isThumbnailCacheRecordUsable(record: ThumbnailCacheRecord, now: number): boolean {
  if (record.state === 'ready') return true
  const ttl = record.state === 'failed' ? THUMBNAIL_FAILED_TTL_MS : THUMBNAIL_NEGATIVE_TTL_MS
  return now - record.touched < ttl
}

/** Expires negatives first, then uses touched time and key as deterministic LRU tie-breakers. */
export function pruneThumbnailCache(
  cache: Record<string, ThumbnailCacheRecord>,
  protectedKeys: ReadonlySet<string>,
  now: number,
): Record<string, ThumbnailCacheRecord> {
  const next = { ...cache }
  for (const [key, record] of Object.entries(next)) {
    if (!protectedKeys.has(key) && !isThumbnailCacheRecordUsable(record, now)) {
      delete next[key]
    }
  }

  let entries = Object.entries(next)
  let weight = entries.reduce((total, [, record]) => total + record.weight, 0)
  const candidates = entries
    .filter(([key]) => !protectedKeys.has(key))
    .sort(
      ([leftKey, left], [rightKey, right]) =>
        left.touched - right.touched || leftKey.localeCompare(rightKey),
    )

  for (const [key, record] of candidates) {
    if (entries.length <= MAX_THUMBNAIL_CACHE_ENTRIES && weight <= MAX_THUMBNAIL_CACHE_BYTES) break
    delete next[key]
    entries = entries.filter(([entryKey]) => entryKey !== key)
    weight -= record.weight
  }
  if (entries.length > MAX_THUMBNAIL_CACHE_ENTRIES || weight > MAX_THUMBNAIL_CACHE_BYTES) {
    const protectedCandidates = Object.entries(next).sort(
      ([leftKey, left], [rightKey, right]) =>
        left.touched - right.touched || leftKey.localeCompare(rightKey),
    )
    for (const [key, record] of protectedCandidates) {
      if (entries.length <= MAX_THUMBNAIL_CACHE_ENTRIES && weight <= MAX_THUMBNAIL_CACHE_BYTES)
        break
      delete next[key]
      entries = entries.filter(([entryKey]) => entryKey !== key)
      weight -= record.weight
    }
  }
  return next
}
