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

type CacheBudget = {
  usage: CacheUsage
  limits: WeightedCacheLimits
}

function evictUntilWithinLimits<CacheRecord extends WeightedCacheRecord>(
  cache: Record<string, CacheRecord>,
  candidates: [string, CacheRecord][],
  { usage, limits }: CacheBudget,
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
export type WeightedCachePolicy<CacheRecord extends WeightedCacheRecord> = {
  isExpired: (record: CacheRecord) => boolean
  limits: WeightedCacheLimits
  evictProtected?: boolean
}

export function pruneWeightedCache<CacheRecord extends WeightedCacheRecord>(
  cache: Record<string, CacheRecord>,
  protectedKeys: ReadonlySet<string>,
  policy: WeightedCachePolicy<CacheRecord>,
): Record<string, CacheRecord> {
  const { isExpired, limits, evictProtected = false } = policy
  const next = { ...cache }
  for (const [key, record] of Object.entries(next)) {
    const evictable = !protectedKeys.has(key) && isExpired(record)
    if (evictable) delete next[key]
  }

  const remaining = Object.entries(next)
  const usage: CacheUsage = {
    count: remaining.length,
    weight: remaining.reduce((total, [, record]) => total + record.weight, 0),
  }
  const unprotected = remaining.filter(([key]) => !protectedKeys.has(key))
  const budget = { usage, limits }
  evictUntilWithinLimits(next, unprotected, budget)
  if (evictProtected) evictUntilWithinLimits(next, Object.entries(next), budget)
  return next
}
