export type WeightedCacheRecord = { touched: number; weight: number }

export type WeightedCacheLimits = { maxEntries: number; maxBytes: number }

type CacheUsage = { count: number; weight: number }

export function cacheNow(): number {
  return Date.now()
}

function oldestFirst<CacheRecord extends WeightedCacheRecord>(
  [leftKey, left]: [string, CacheRecord],
  [rightKey, right]: [string, CacheRecord],
) {
  return left.touched - right.touched || leftKey.localeCompare(rightKey)
}

function withinLimits(usage: CacheUsage, limits: WeightedCacheLimits) {
  return usage.count <= limits.maxEntries && usage.weight <= limits.maxBytes
}

function evictUntilWithinLimits<CacheRecord extends WeightedCacheRecord>(
  cache: Record<string, CacheRecord>,
  candidates: [string, CacheRecord][],
  usage: CacheUsage,
  limits: WeightedCacheLimits,
) {
  for (const [key, record] of candidates.sort(oldestFirst)) {
    if (withinLimits(usage, limits)) return
    delete cache[key]
    usage.count -= 1
    usage.weight -= record.weight
  }
}

/**
 * Drops expired unprotected records, then evicts the deterministic oldest
 * unprotected records until both limits are met. With `evictProtected`, the
 * oldest protected records go too when unprotected eviction was not enough.
 */
export function pruneWeightedCache<CacheRecord extends WeightedCacheRecord>(
  cache: Record<string, CacheRecord>,
  protectedKeys: ReadonlySet<string>,
  isExpired: (record: CacheRecord) => boolean,
  limits: WeightedCacheLimits,
  evictProtected = false,
): Record<string, CacheRecord> {
  const next = { ...cache }
  for (const [key, record] of Object.entries(next)) {
    if (!protectedKeys.has(key) && isExpired(record)) delete next[key]
  }

  const remaining = Object.entries(next)
  const usage: CacheUsage = {
    count: remaining.length,
    weight: remaining.reduce((total, [, record]) => total + record.weight, 0),
  }
  const unprotected = remaining.filter(([key]) => !protectedKeys.has(key))
  evictUntilWithinLimits(next, unprotected, usage, limits)
  if (evictProtected) evictUntilWithinLimits(next, Object.entries(next), usage, limits)
  return next
}
