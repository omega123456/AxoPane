import {
  buildTreeFlatModel,
  treeRowHeight,
  TREE_HEADER_HEIGHT_PX,
  TREE_ROW_HEIGHT_PX,
} from '@/lib/tree-flat'
import { TRASH_PATH } from '@/lib/trash'
import type { VolumeInfo } from '@/lib/types/ipc'
import type { TreeNodeState } from '@/stores/panes-store'

function volume(overrides: Partial<VolumeInfo> & Pick<VolumeInfo, 'mountRoot'>): VolumeInfo {
  return {
    label: 'Vol',
    totalBytes: 1,
    freeBytes: 1,
    isNetwork: false,
    isRemovable: false,
    ...overrides,
  }
}

function node(overrides: Partial<TreeNodeState> & Pick<TreeNodeState, 'path'>): TreeNodeState {
  return {
    id: overrides.path,
    name: overrides.path,
    parentPath: null,
    children: [],
    expanded: false,
    loaded: true,
    ...overrides,
  }
}

describe('buildTreeFlatModel', () => {
  it('emits a heading per non-empty category, each volume subtree, and trash after the fixed group', () => {
    const volumes = [
      volume({ mountRoot: 'C:\\', isRemovable: false }),
      volume({ mountRoot: 'E:\\', isRemovable: true }),
      volume({ mountRoot: 'Z:\\', isNetwork: true }),
    ]
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', children: ['C:\\aa'], expanded: true }),
      'C:\\aa': node({ path: 'C:\\aa', parentPath: 'C:\\' }),
      'E:\\': node({ path: 'E:\\' }),
      'Z:\\': node({ path: 'Z:\\' }),
    }

    const { rows } = buildTreeFlatModel(treeNodes, volumes)

    expect(rows).toEqual([
      { kind: 'header', label: 'Drives' },
      { kind: 'node', path: 'C:\\', depth: 0, volume: volumes[0] },
      { kind: 'node', path: 'C:\\aa', depth: 1, volume: undefined },
      { kind: 'trash' },
      { kind: 'header', label: 'Removable Drives' },
      { kind: 'node', path: 'E:\\', depth: 0, volume: volumes[1] },
      { kind: 'header', label: 'Network Drives' },
      { kind: 'node', path: 'Z:\\', depth: 0, volume: volumes[2] },
    ])
  })

  it('excludes descendants of collapsed nodes', () => {
    const volumes = [volume({ mountRoot: 'C:\\' })]
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', children: ['C:\\aa'], expanded: false }),
      'C:\\aa': node({ path: 'C:\\aa', parentPath: 'C:\\', children: ['C:\\aa\\bb'], expanded: true }),
      'C:\\aa\\bb': node({ path: 'C:\\aa\\bb', parentPath: 'C:\\aa' }),
    }

    const { rows } = buildTreeFlatModel(treeNodes, volumes)

    expect(rows.filter((row) => row.kind === 'node')).toEqual([
      { kind: 'node', path: 'C:\\', depth: 0, volume: volumes[0] },
    ])
  })

  it('contributes no rows for a volume whose root node is not yet loaded', () => {
    const volumes = [volume({ mountRoot: 'C:\\' })]

    const { rows } = buildTreeFlatModel({}, volumes)

    expect(rows).toEqual([{ kind: 'header', label: 'Drives' }, { kind: 'trash' }])
  })

  it('computes cumulative offsets by row kind, ending with the total height', () => {
    const volumes = [volume({ mountRoot: 'C:\\' })]
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', children: ['C:\\aa'], expanded: true }),
      'C:\\aa': node({ path: 'C:\\aa', parentPath: 'C:\\' }),
    }

    const { rows, offsets } = buildTreeFlatModel(treeNodes, volumes)

    // header, C:\, C:\aa, trash
    expect(offsets).toHaveLength(rows.length + 1)
    expect(offsets[0]).toBe(0)
    for (let i = 0; i < rows.length; i += 1) {
      expect(offsets[i + 1] - offsets[i]).toBeCloseTo(treeRowHeight(rows[i]), 5)
    }
    // Total height ends at header + three fixed-height rows (C:\, C:\aa, trash).
    expect(offsets[rows.length]).toBeCloseTo(TREE_HEADER_HEIGHT_PX + TREE_ROW_HEIGHT_PX * 3, 5)
  })

  it('maps every node path and the trash sentinel to its flat index', () => {
    const volumes = [volume({ mountRoot: 'C:\\' })]
    const treeNodes: Record<string, TreeNodeState> = {
      'C:\\': node({ path: 'C:\\', children: ['C:\\aa'], expanded: true }),
      'C:\\aa': node({ path: 'C:\\aa', parentPath: 'C:\\' }),
    }

    const { indexByPath } = buildTreeFlatModel(treeNodes, volumes)

    expect(indexByPath.get('C:\\')).toBe(1)
    expect(indexByPath.get('C:\\aa')).toBe(2)
    expect(indexByPath.get(TRASH_PATH)).toBe(3)
  })

  it('sizes header rows and node/trash rows from their tokens', () => {
    expect(treeRowHeight({ kind: 'header', label: 'Drives' })).toBe(TREE_HEADER_HEIGHT_PX)
    expect(treeRowHeight({ kind: 'node', path: 'C:\\', depth: 0 })).toBe(TREE_ROW_HEIGHT_PX)
    expect(treeRowHeight({ kind: 'trash' })).toBe(TREE_ROW_HEIGHT_PX)
  })
})
