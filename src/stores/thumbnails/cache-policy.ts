export const MAX_THUMBNAIL_CACHE_ENTRIES = 256
export const MAX_THUMBNAIL_CACHE_BYTES = 32 * 1024 * 1024
export const THUMBNAIL_NEGATIVE_TTL_MS = 5 * 60 * 1000

export type ThumbnailCacheState = 'ready' | 'unavailable' | 'failed'

export type ThumbnailCacheRecord = {
  state: ThumbnailCacheState
  dataUrl: string | null
  touched: number
  weight: number
}

/** JavaScript stores strings as UTF-16, so this is a conservative cache estimate. */
export function thumbnailWeight(dataUrl: string | null): number {
  return dataUrl == null ? 1 : dataUrl.length * 2
}

export function thumbnailCacheNow(): number {
  return Date.now()
}

export function isThumbnailCacheRecordUsable(record: ThumbnailCacheRecord, now: number): boolean {
  return record.state === 'ready' || now - record.touched < THUMBNAIL_NEGATIVE_TTL_MS
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
    .sort(([leftKey, left], [rightKey, right]) => left.touched - right.touched || leftKey.localeCompare(rightKey))

  for (const [key, record] of candidates) {
    if (entries.length <= MAX_THUMBNAIL_CACHE_ENTRIES && weight <= MAX_THUMBNAIL_CACHE_BYTES) break
    delete next[key]
    entries = entries.filter(([entryKey]) => entryKey !== key)
    weight -= record.weight
  }
  return next
}
