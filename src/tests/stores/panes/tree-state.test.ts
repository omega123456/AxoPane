import { describe, expect, it } from 'vitest'
import type { VolumeInfo } from '@/lib/types/ipc'
import type { TreeNodeState } from '@/stores/panes/pane-state'
import {
  buildRootTreeState,
  failedTreeNodes,
  loadedTreeNodes,
  revealChain,
  revealedTree,
  toggledTreeNode,
  treeRootForPath,
  withHiddenLevelsShown,
  withRevealedSiblings,
  withVolumes,
} from '@/stores/panes/tree-state'
import { panesData } from './pane-fixtures'

function volume(mountRoot: string): VolumeInfo {
  return {
    mountRoot,
    label: mountRoot,
    totalBytes: 1,
    freeBytes: 1,
    isNetwork: false,
    isRemovable: false,
  }
}

function node(path: string, overrides: Partial<TreeNodeState> = {}): TreeNodeState {
  return {
    id: path,
    name: path.split('/').at(-1) || path,
    path,
    parentPath: null,
    children: [],
    expanded: false,
    loaded: false,
    ...overrides,
  }
}

const CLOSED_FOLD = { leaf: null, revealedSiblings: [], showHiddenLevels: false }

describe('tree state', () => {
  it('rebuilds volume roots while keeping cached descendants and root state', () => {
    const current = {
      '/': node('/', { expanded: true, loaded: true, children: ['/Users'] }),
      '/Users': node('/Users', { parentPath: '/' }),
      '/Volumes/Gone/x': node('/Volumes/Gone/x', { parentPath: '/Volumes/Gone' }),
    }
    const tree = buildRootTreeState([volume('/')], current)
    expect(tree.treeRoots).toEqual(['/'])
    expect(tree.treeNodes['/']).toMatchObject({ expanded: true, loaded: true })
    expect(tree.treeNodes['/Users']).toBeDefined()
    expect(treeRootForPath(tree.treeRoots, '/')).toBe('/')
    expect(treeRootForPath(tree.treeRoots, '/nope')).toBeNull()

    const fresh = withVolumes(panesData(), [volume('/')])
    expect(fresh.treeNodes['/']).toMatchObject({ loaded: false, expandability: 'unknown' })
  })

  it('records loaded children and failures for a node', () => {
    const tree = { treeRoots: ['/'], treeNodes: { '/': node('/') } }
    const children = [{ name: 'Users', path: '/Users', expandability: 'nonEmpty' as const }]
    const loaded = loadedTreeNodes(tree, '/', tree.treeNodes['/'], children)
    expect(loaded['/']).toMatchObject({ loaded: true, expanded: true, children: ['/Users'] })
    expect(loaded['/Users']).toMatchObject({ parentPath: '/', expandability: 'nonEmpty' })

    const relisted = loadedTreeNodes({ ...tree, treeNodes: loaded }, '/', loaded['/'], [
      { ...children[0], name: 'Renamed' },
    ])
    expect(relisted['/Users'].name).toBe('Renamed')

    const failed = failedTreeNodes(tree, '/Users', undefined)
    expect(failed['/Users']).toMatchObject({ loaded: true, expanded: false, parentPath: '/' })
  })

  it('reveals the ancestor chain and collapses unrelated branches', () => {
    expect(revealChain('/a/b/c', '/')).toEqual(['/', '/a', '/a/b', '/a/b/c'])
    const state = {
      treeFold: CLOSED_FOLD,
      treeNodes: {
        '/': node('/'),
        '/a': node('/a'),
        '/other': node('/other', { expanded: true }),
        '/a/b': node('/a/b', { expanded: true }),
      },
    }
    const revealed = revealedTree(state, '/a/b', ['/', '/a', '/a/b'])
    expect(revealed.treeNodes['/'].expanded).toBe(true)
    expect(revealed.treeNodes['/a'].expanded).toBe(true)
    expect(revealed.treeNodes['/other'].expanded).toBe(false)
    expect(revealed.treeNodes['/a/b'].expanded).toBe(true)
    expect(revealed.treeFold.leaf).toBe('/a/b')
    expect(revealedTree(revealed, '/a/b', ['/', '/a', '/a/b']).treeFold).toBe(revealed.treeFold)
  })

  it('toggles nodes, creating a placeholder for unseen paths', () => {
    expect(toggledTreeNode({ treeNodes: {} }, '/new/dir').treeNodes['/new/dir']).toMatchObject({
      expanded: true,
      name: 'dir',
      parentPath: '/new',
    })
    const existing = { treeNodes: { '/x': node('/x', { expanded: true }) } }
    expect(toggledTreeNode(existing, '/x').treeNodes['/x'].expanded).toBe(false)
  })

  it('reveals siblings once and shows hidden levels', () => {
    const revealed = withRevealedSiblings({ treeFold: CLOSED_FOLD }, '/a')
    expect(revealed?.treeFold.revealedSiblings).toEqual(['/a'])
    expect(withRevealedSiblings({ treeFold: revealed!.treeFold }, '/a')).toBeNull()
    expect(withHiddenLevelsShown({ treeFold: CLOSED_FOLD }).treeFold.showHiddenLevels).toBe(true)
  })
})
