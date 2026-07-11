import type { TreeChildEntry, TreeExpandability } from '@/lib/types/ipc'
import { pathsMatch } from '@/lib/path-compare'

export const TREE_CACHE_MAX_UNPINNED_NODES = 20_000

export type LazyTreeNode = {
  id: string
  name: string
  path: string
  parentPath: string | null
  children: string[]
  expanded: boolean
  loaded: boolean
  expandability: TreeExpandability
  lastAccess: number
}

/** Shares equivalent node loads while allowing a later completed load to start afresh. */
export class TreeSingleFlight {
  private readonly flights = new Map<string, Promise<void>>()

  run(path: string, load: () => Promise<void>): Promise<void> {
    const existing = this.flights.get(path)
    if (existing) return existing
    const flight = load().finally(() => this.flights.delete(path))
    this.flights.set(path, flight)
    return flight
  }
}

export function expandabilityOf(entry: Pick<TreeChildEntry, 'expandability'>): TreeExpandability {
  return entry.expandability
}

export function pruneTreeCache<T extends LazyTreeNode>(
  nodes: Record<string, T>,
  protectedPaths: readonly string[],
  maxNodes = TREE_CACHE_MAX_UNPINNED_NODES,
): Record<string, T> {
  if (Object.keys(nodes).length <= maxNodes) return nodes
  const protectedKeys = new Set(
    Object.keys(nodes).filter((path) =>
      protectedPaths.some((protectedPath) => pathsMatch(path, protectedPath)),
    ),
  )
  const candidates = Object.entries(nodes)
    .filter(([path]) => !protectedKeys.has(path))
    .sort(([, left], [, right]) => left.lastAccess - right.lastAccess)
  const next = { ...nodes }
  let remaining = Object.keys(next).length
  for (const [path] of candidates) {
    if (remaining <= maxNodes) break
    delete next[path]
    remaining -= 1
  }
  return next
}

/** Removes a cached node and every cached descendant, without touching an unloaded branch. */
export function pruneCachedSubtree<T extends Pick<LazyTreeNode, 'children'>>(
  nodes: Record<string, T>,
  path: string,
): Record<string, T> {
  const root = Object.keys(nodes).find((candidate) => pathsMatch(candidate, path))
  if (!root) return nodes
  const next = { ...nodes }
  const pending = [root]
  while (pending.length > 0) {
    const current = pending.pop()!
    pending.push(...(next[current]?.children ?? []))
    delete next[current]
  }
  return next
}
