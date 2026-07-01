import { buildStickyChain, flattenVisibleTree } from '@/lib/tree-sticky'
import type { TreeNodeState } from '@/stores/panes-store'

function node(overrides: Partial<TreeNodeState> & Pick<TreeNodeState, 'path'>): TreeNodeState {
  return {
    id: overrides.path,
    name: overrides.path,
    parentPath: null,
    children: [],
    expanded: true,
    loaded: true,
    ...overrides,
  }
}

describe('buildStickyChain', () => {
  it('returns an empty chain when there is no active path', () => {
    expect(buildStickyChain({}, null)).toEqual([])
  })

  it('returns an empty chain when the active path has no tree node', () => {
    expect(buildStickyChain({}, 'C:\\missing')).toEqual([])
  })

  it('walks parentPath links from the active node up to the volume root', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', parentPath: null }),
      'C:\\aa': node({ path: 'C:\\aa', parentPath: 'C:\\' }),
      'C:\\aa\\bb': node({ path: 'C:\\aa\\bb', parentPath: 'C:\\aa' }),
    }

    expect(buildStickyChain(treeNodes, 'C:\\aa\\bb')).toEqual(['C:\\', 'C:\\aa', 'C:\\aa\\bb'])
  })

  it('stops walking once a node has no parentPath', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', parentPath: null }),
    }

    expect(buildStickyChain(treeNodes, 'C:\\')).toEqual(['C:\\'])
  })

  it('walks the full chain even when many ancestor levels are revealed in one jump', () => {
    // Regression: navigating straight to a deeply nested path (e.g. via a
    // bookmark) reveals every ancestor's tree node in a single state update,
    // not incrementally - the chain must include all of them, not just the
    // first few, or deeper rows never get their own sticky offset.
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', parentPath: null }),
      'C:\\a': node({ path: 'C:\\a', parentPath: 'C:\\' }),
      'C:\\a\\b': node({ path: 'C:\\a\\b', parentPath: 'C:\\a' }),
      'C:\\a\\b\\c': node({ path: 'C:\\a\\b\\c', parentPath: 'C:\\a\\b' }),
      'C:\\a\\b\\c\\d': node({ path: 'C:\\a\\b\\c\\d', parentPath: 'C:\\a\\b\\c' }),
      'C:\\a\\b\\c\\d\\e': node({ path: 'C:\\a\\b\\c\\d\\e', parentPath: 'C:\\a\\b\\c\\d' }),
    }

    expect(buildStickyChain(treeNodes, 'C:\\a\\b\\c\\d\\e')).toEqual([
      'C:\\',
      'C:\\a',
      'C:\\a\\b',
      'C:\\a\\b\\c',
      'C:\\a\\b\\c\\d',
      'C:\\a\\b\\c\\d\\e',
    ])
  })
})

describe('flattenVisibleTree', () => {
  it('returns the root and expanded descendants in depth-first render order', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({
        path: 'C:\\',
        parentPath: null,
        children: ['C:\\aa', 'C:\\cc'],
        expanded: true,
      }),
      'C:\\aa': node({
        path: 'C:\\aa',
        parentPath: 'C:\\',
        children: ['C:\\aa\\bb'],
        expanded: true,
      }),
      'C:\\aa\\bb': node({
        path: 'C:\\aa\\bb',
        parentPath: 'C:\\aa',
        children: [],
        expanded: false,
      }),
      'C:\\cc': node({
        path: 'C:\\cc',
        parentPath: 'C:\\',
        children: [],
        expanded: false,
      }),
    }

    expect(flattenVisibleTree(treeNodes, 'C:\\')).toEqual([
      { path: 'C:\\', depth: 0 },
      { path: 'C:\\aa', depth: 1 },
      { path: 'C:\\aa\\bb', depth: 2 },
      { path: 'C:\\cc', depth: 1 },
    ])
  })

  it('does not include descendants under collapsed nodes', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({
        path: 'C:\\',
        parentPath: null,
        children: ['C:\\aa'],
        expanded: true,
      }),
      'C:\\aa': node({
        path: 'C:\\aa',
        parentPath: 'C:\\',
        children: ['C:\\aa\\bb'],
        expanded: false,
      }),
      'C:\\aa\\bb': node({
        path: 'C:\\aa\\bb',
        parentPath: 'C:\\aa',
        children: [],
        expanded: true,
      }),
    }

    expect(flattenVisibleTree(treeNodes, 'C:\\')).toEqual([
      { path: 'C:\\', depth: 0 },
      { path: 'C:\\aa', depth: 1 },
    ])
  })

  it('returns an empty list for a missing root node', () => {
    expect(flattenVisibleTree({}, 'C:\\missing')).toEqual([])
  })
})
