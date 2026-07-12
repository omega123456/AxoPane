/** Deterministic bounded metadata cache policies used by pane state. */
export const MAX_SIZE_CACHE_ENTRIES = 10_000
export const MAX_ICON_CACHE_ENTRIES = 4_096
export const MAX_ICON_CACHE_BYTES = 32 * 1024 * 1024
export const ICON_NEGATIVE_TTL_MS = 10 * 60 * 1000

export type IconCacheRecord = { value: string | null; touched: number; weight: number }

export function iconWeight(value: string | null): number {
  return value == null ? 1 : value.length * 2
}

export function iconCacheNow(): number {
  return Date.now()
}

/**
 * Retains protected records, then evicts the deterministic oldest unprotected
 * records until both approved limits are met. Negative records expire first.
 */
export function pruneIconCache(
  cache: Record<string, IconCacheRecord>,
  protectedPaths: ReadonlySet<string>,
  now: number,
  nativeIconsSupported: boolean,
): Record<string, IconCacheRecord> {
  const next = { ...cache }
  for (const [path, record] of Object.entries(next)) {
    if (
      record.value === null &&
      (!nativeIconsSupported || now - record.touched >= ICON_NEGATIVE_TTL_MS) &&
      !protectedPaths.has(path)
    )
      delete next[path]
  }
  const remaining = Object.entries(next)
  let count = remaining.length
  let weight = remaining.reduce((total, [, record]) => total + record.weight, 0)
  const candidates = remaining
    .filter(([path]) => !protectedPaths.has(path))
    .sort(
      ([leftPath, left], [rightPath, right]) =>
        left.touched - right.touched || leftPath.localeCompare(rightPath),
    )
  for (const [path, record] of candidates) {
    if (count <= MAX_ICON_CACHE_ENTRIES && weight <= MAX_ICON_CACHE_BYTES) break
    delete next[path]
    count -= 1
    weight -= record.weight
  }
  return next
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
