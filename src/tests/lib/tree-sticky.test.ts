import {
  buildStickyChain,
  capStickyChain,
  flattenVisibleTree,
  type TreeFoldState,
} from '@/lib/tree-sticky'
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
      { kind: 'node', path: 'C:\\', depth: 0 },
      { kind: 'node', path: 'C:\\aa', depth: 1 },
      { kind: 'node', path: 'C:\\aa\\bb', depth: 2 },
      { kind: 'node', path: 'C:\\cc', depth: 1 },
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
      { kind: 'node', path: 'C:\\', depth: 0 },
      { kind: 'node', path: 'C:\\aa', depth: 1 },
    ])
  })

  it('does not render nested mounted volume roots from an ancestor volume', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      '/': node({
        path: '/',
        parentPath: null,
        children: ['/Volumes'],
        expanded: true,
      }),
      '/Volumes': node({
        path: '/Volumes',
        parentPath: '/',
        children: ['/Volumes/team'],
        expanded: true,
      }),
      '/Volumes/team': node({
        path: '/Volumes/team',
        parentPath: null,
        children: ['/Volumes/team/Project'],
        expanded: true,
      }),
      '/Volumes/team/Project': node({
        path: '/Volumes/team/Project',
        parentPath: '/Volumes/team',
        children: [],
        expanded: false,
      }),
    }

    expect(flattenVisibleTree(treeNodes, '/', new Set(['/volumes/team']))).toEqual([
      { kind: 'node', path: '/', depth: 0 },
      { kind: 'node', path: '/Volumes', depth: 1 },
    ])
    expect(flattenVisibleTree(treeNodes, '/Volumes/team', new Set(['/']))).toEqual([
      { kind: 'node', path: '/Volumes/team', depth: 0 },
      { kind: 'node', path: '/Volumes/team/Project', depth: 1 },
    ])
  })

  it('returns an empty list for a missing root node', () => {
    expect(flattenVisibleTree({}, 'C:\\missing')).toEqual([])
  })
})

/**
 * A `levels`-deep active path under `C:\` where every path folder sits between
 * two siblings (`a` before, `z` after). Path folder `n` is `p1\p2\...\pn`.
 */
function pathTree(levels: number) {
  const chain = ['C:\\']
  for (let level = 1; level <= levels; level += 1) {
    chain.push(`${chain[level - 1].replace(/\\$/, '')}\\p${level}`)
  }
  const treeNodes: Record<string, TreeNodeState> = {}
  chain.forEach((path, index) => {
    const next = chain[index + 1]
    const children = next
      ? [`${path.replace(/\\$/, '')}\\a`, next, `${path.replace(/\\$/, '')}\\z`]
      : []
    treeNodes[path] = node({
      path,
      parentPath: chain[index - 1] ?? null,
      children,
      expanded: next !== undefined,
    })
    for (const sibling of children.filter((child) => child !== next)) {
      treeNodes[sibling] = node({ path: sibling, parentPath: path, expanded: false })
    }
  })
  return { chain, treeNodes }
}

function foldOf(chain: string[], overrides: Partial<TreeFoldState> = {}): TreeFoldState {
  return { chain, revealedSiblings: new Set(), showHiddenLevels: false, ...overrides }
}

describe('flattenVisibleTree folding', () => {
  it('shows each path folder followed by one other-folders row per level', () => {
    const { chain, treeNodes } = pathTree(2)

    expect(flattenVisibleTree(treeNodes, 'C:\\', new Set(), foldOf(chain))).toEqual([
      { kind: 'node', path: 'C:\\', depth: 0 },
      { kind: 'node', path: 'C:\\p1', depth: 1 },
      { kind: 'node', path: 'C:\\p1\\p2', depth: 2 },
      { kind: 'fold-siblings', parentPath: 'C:\\p1', count: 2, depth: 2 },
      { kind: 'fold-siblings', parentPath: 'C:\\', count: 2, depth: 1 },
    ])
  })

  it('omits the other-folders row when the path folder has no siblings', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', children: ['C:\\only'] }),
      'C:\\only': node({ path: 'C:\\only', parentPath: 'C:\\', expanded: false }),
    }

    expect(flattenVisibleTree(treeNodes, 'C:\\', new Set(), foldOf(['C:\\', 'C:\\only']))).toEqual([
      { kind: 'node', path: 'C:\\', depth: 0 },
      { kind: 'node', path: 'C:\\only', depth: 1 },
    ])
  })

  it('lists every child of a revealed path folder and keeps folding below it', () => {
    const { chain, treeNodes } = pathTree(2)
    const fold = foldOf(chain, { revealedSiblings: new Set(['C:\\']) })

    expect(flattenVisibleTree(treeNodes, 'C:\\', new Set(), fold)).toEqual([
      { kind: 'node', path: 'C:\\', depth: 0 },
      { kind: 'node', path: 'C:\\a', depth: 1 },
      { kind: 'node', path: 'C:\\p1', depth: 1 },
      { kind: 'node', path: 'C:\\p1\\p2', depth: 2 },
      { kind: 'fold-siblings', parentPath: 'C:\\p1', count: 2, depth: 2 },
      { kind: 'node', path: 'C:\\z', depth: 1 },
    ])
  })

  it('walks every child when the next path folder is not among the loaded children', () => {
    const { chain, treeNodes } = pathTree(2)
    treeNodes['C:\\'] = { ...treeNodes['C:\\'], children: ['C:\\a', 'C:\\z'] }

    expect(flattenVisibleTree(treeNodes, 'C:\\', new Set(), foldOf(chain))).toEqual([
      { kind: 'node', path: 'C:\\', depth: 0 },
      { kind: 'node', path: 'C:\\a', depth: 1 },
      { kind: 'node', path: 'C:\\z', depth: 1 },
    ])
  })

  it('does not fold toward a nested volume root excluded from this volume', () => {
    const treeNodes: Record<string, TreeNodeState> = {
      '/': node({ path: '/', children: ['/Volumes'] }),
      '/Volumes': node({ path: '/Volumes', parentPath: '/', children: ['/Volumes/team'] }),
      '/Volumes/team': node({ path: '/Volumes/team', parentPath: '/Volumes', expanded: false }),
    }
    const fold = foldOf(['/', '/Volumes', '/Volumes/team'])

    expect(flattenVisibleTree(treeNodes, '/', new Set(['/volumes/team']), fold)).toEqual([
      { kind: 'node', path: '/', depth: 0 },
      { kind: 'node', path: '/Volumes', depth: 1 },
    ])
  })

  it('folds the middle of a path deeper than six levels into one row', () => {
    const { chain, treeNodes } = pathTree(10)

    const rows = flattenVisibleTree(treeNodes, 'C:\\', new Set(), foldOf(chain))

    expect(rows.slice(0, 5)).toEqual([
      { kind: 'node', path: chain[0], depth: 0 },
      { kind: 'node', path: chain[1], depth: 1 },
      { kind: 'node', path: chain[2], depth: 2 },
      { kind: 'fold-levels', hiddenPaths: chain.slice(3, 7), depth: 3 },
      { kind: 'node', path: chain[7], depth: 7 },
    ])
    expect(rows.filter((row) => row.kind === 'node').map((row) => row.path)).toEqual([
      ...chain.slice(0, 3),
      ...chain.slice(7),
    ])
    const foldedParents = rows.flatMap((row) =>
      row.kind === 'fold-siblings' ? [row.parentPath] : [],
    )
    expect(foldedParents).toEqual([chain[9], chain[8], chain[7], chain[2], chain[1], chain[0]])
  })

  it('folds hidden levels even when the folder above them is revealed', () => {
    const { chain, treeNodes } = pathTree(10)
    const fold = foldOf(chain, { revealedSiblings: new Set([chain[2]]) })

    const rows = flattenVisibleTree(treeNodes, 'C:\\', new Set(), fold)

    expect(rows.find((row) => row.kind === 'fold-levels')).toEqual({
      kind: 'fold-levels',
      hiddenPaths: chain.slice(3, 7),
      depth: 3,
    })
    expect(rows.some((row) => row.kind === 'node' && row.path === 'C:\\p1\\p2\\a')).toBe(true)
  })

  it('shows every level once hidden levels are revealed', () => {
    const { chain, treeNodes } = pathTree(10)

    const rows = flattenVisibleTree(
      treeNodes,
      'C:\\',
      new Set(),
      foldOf(chain, { showHiddenLevels: true }),
    )

    expect(rows.some((row) => row.kind === 'fold-levels')).toBe(false)
    expect(rows.filter((row) => row.kind === 'node')).toHaveLength(chain.length)
  })

  it('keeps every level when a hidden level is collapsed', () => {
    const { chain, treeNodes } = pathTree(10)
    treeNodes[chain[4]] = { ...treeNodes[chain[4]], expanded: false }

    const rows = flattenVisibleTree(treeNodes, 'C:\\', new Set(), foldOf(chain))

    expect(rows.some((row) => row.kind === 'fold-levels')).toBe(false)
    expect(rows.filter((row) => row.kind === 'node').map((row) => row.path)).toEqual(
      chain.slice(0, 5),
    )
  })
})

describe('capStickyChain', () => {
  const index = (paths: string[]) => new Map(paths.map((path, position) => [path, position]))

  it('keeps a short chain unchanged', () => {
    const chain = ['C:\\', 'C:\\a', 'C:\\a\\b', 'C:\\a\\b\\c']
    expect(capStickyChain(chain, index(chain))).toEqual(chain)
  })

  it('keeps the volume root and the three nearest path folders', () => {
    const { chain } = pathTree(6)
    expect(capStickyChain(chain, index(chain))).toEqual([chain[0], ...chain.slice(-3)])
  })

  it('skips path folders that are folded out of the tree', () => {
    const chain = ['C:\\', 'C:\\a', 'C:\\a\\b']
    expect(capStickyChain(chain, index(['C:\\', 'C:\\a\\b']))).toEqual(['C:\\', 'C:\\a\\b'])
  })
})
