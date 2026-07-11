import { describe, expect, it } from 'vitest'
import {
  expandabilityOf,
  pruneCachedSubtree,
  pruneTreeCache,
  TreeSingleFlight,
  type LazyTreeNode,
} from '@/stores/panes/tree-cache'

function node(path: string, lastAccess: number, children: string[] = []): LazyTreeNode {
  return {
    id: path,
    name: path,
    path,
    parentPath: null,
    children,
    expanded: false,
    loaded: true,
    expandability: 'unknown',
    lastAccess,
  }
}

describe('tree-cache', () => {
  it('uses the authoritative tri-state expandability contract', () => {
    expect(expandabilityOf({ expandability: 'empty' })).toBe('empty')
    expect(expandabilityOf({ expandability: 'nonEmpty' })).toBe('nonEmpty')
  })

  it('evicts least-recent unprotected nodes at the bounded limit', () => {
    const nodes = { a: node('a', 1), b: node('b', 2), c: node('c', 3) }
    expect(Object.keys(pruneTreeCache(nodes, ['a'], 2)).sort()).toEqual(['a', 'c'])
  })

  it('prunes an accepted removed directory cached subtree only', () => {
    const nodes = {
      root: node('root', 1, ['removed', 'kept']),
      removed: node('removed', 2, ['nested']),
      nested: node('nested', 3),
      kept: node('kept', 4),
    }
    expect(Object.keys(pruneCachedSubtree(nodes, 'removed')).sort()).toEqual(['kept', 'root'])
  })

  it('shares an equivalent in-flight expansion request', async () => {
    const singleFlight = new TreeSingleFlight()
    let calls = 0
    let finish: () => void = () => undefined
    const load = () => {
      calls += 1
      return new Promise<void>((resolve) => {
        finish = resolve
      })
    }
    const first = singleFlight.run('C:\\tree', load)
    const second = singleFlight.run('C:\\tree', load)
    expect(calls).toBe(1)
    expect(second).toBe(first)
    finish()
    await first
  })
})
