import type { VolumeInfo } from '@/lib/types/ipc'
import type { TreeNodeState } from '@/stores/panes-store'
import { TRASH_PATH } from '@/lib/trash'
import { flattenVisibleTree } from '@/lib/tree-sticky'
import { groupVolumesByCategory } from '@/lib/volumes'

/**
 * Pixel geometry of the virtualized tree, derived from the fixed `@theme`
 * tokens `--spacing-tree-row` (1.5625rem = 25px) and
 * `--spacing-tree-header` (1.75rem = 28px) at the app's 16px root. These must
 * stay compile-time constants: the virtualizer sizes each flat row by kind
 * from them and the sticky overlay's stacking offsets are multiples of the row
 * height. The row height also stays on a whole CSS pixel so Chromium and
 * WebKit accumulate the same offsets in long trees.
 */
export const TREE_ROW_HEIGHT_PX = 25
export const TREE_HEADER_HEIGHT_PX = 28

/**
 * One row of the fully-flattened tree. A single ordered array of these spans
 * every category section, every volume subtree and the trash row, so the whole
 * tree can be driven by one virtualizer (only the visible window mounts) rather
 * than a real `<TreeNode>` per row.
 */
export type TreeFlatRow =
  | { kind: 'header'; label: string }
  | { kind: 'node'; path: string; depth: number; volume?: VolumeInfo }
  | { kind: 'trash' }

export type TreeFlatModel = {
  rows: TreeFlatRow[]
  /**
   * Cumulative pixel offset of each row from the top of the scroll content
   * (`offsets[i]` is the top of row `i`; `offsets[rows.length]` is the total
   * height). Precomputed here because row heights vary by kind, so the sticky
   * overlay cannot derive an ancestor's natural top from `index * rowHeight`.
   */
  offsets: number[]
  /**
   * `path -> flat index` for every node row (plus `TRASH_PATH`). Drives
   * `scrollToIndex` for the active node and maps each pinned ancestor to its
   * row so the overlay can read its natural offset.
   */
  indexByPath: Map<string, number>
}

/** Height in pixels of a single flat row, keyed on its kind. */
export function treeRowHeight(row: TreeFlatRow): number {
  return row.kind === 'header' ? TREE_HEADER_HEIGHT_PX : TREE_ROW_HEIGHT_PX
}

/**
 * Flatten the store's volumes + tree nodes into one ordered row array in render
 * order: a heading per non-empty category, then each volume's visible subtree
 * (root + expanded descendants via `flattenVisibleTree`), with the trash row
 * appended after the `fixed` group — matching the pre-virtualization layout.
 */
export function buildTreeFlatModel(
  treeNodes: Record<string, TreeNodeState>,
  volumes: VolumeInfo[],
): TreeFlatModel {
  const groups = groupVolumesByCategory(volumes)
  const rows: TreeFlatRow[] = []
  const indexByPath = new Map<string, number>()

  for (const group of groups) {
    rows.push({ kind: 'header', label: group.label })
    for (const volume of group.volumes) {
      for (const row of flattenVisibleTree(treeNodes, volume.mountRoot)) {
        indexByPath.set(row.path, rows.length)
        rows.push({
          kind: 'node',
          path: row.path,
          depth: row.depth,
          volume: row.depth === 0 ? volume : undefined,
        })
      }
    }
    if (group.category === 'fixed') {
      indexByPath.set(TRASH_PATH, rows.length)
      rows.push({ kind: 'trash' })
    }
  }

  const offsets = new Array<number>(rows.length + 1)
  offsets[0] = 0
  for (let i = 0; i < rows.length; i += 1) {
    offsets[i + 1] = offsets[i] + treeRowHeight(rows[i])
  }

  return { rows, offsets, indexByPath }
}
