import type { VolumeInfo } from '@/lib/types/ipc'
import type { PlatformOs } from '@/lib/keymap'
import type { TreeNodeState } from '@/stores/panes-store'
import { TRASH_PATH } from '@/lib/trash'
import { EMPTY_TREE_FOLD, flattenVisibleTree, type TreeFoldState } from '@/lib/tree-sticky'
import { groupVolumesByCategory, volumeUsedFraction } from '@/lib/volumes'

/**
 * Pixel geometry of the virtualized tree, derived from the fixed `@theme`
 * tokens `--spacing-tree-row` (28px), `--spacing-tree-drive-row` (38px, a drive
 * with its capacity bar) and `--spacing-tree-header` (30px) at the app's 16px
 * root. These must stay compile-time constants: the virtualizer sizes each flat
 * row by kind from them and the sticky overlay stacks pinned rows by these
 * heights. Every height is a whole CSS pixel so Chromium and WebKit accumulate
 * the same offsets in long trees.
 */
export const TREE_ROW_HEIGHT_PX = 28
export const TREE_DRIVE_ROW_HEIGHT_PX = 38
export const TREE_HEADER_HEIGHT_PX = 30

/** Depth after which rows stop indenting, so deep names stay readable in a narrow sidebar. */
export const TREE_INDENT_MAX_DEPTH = 4

/** Left padding of a tree row at `depth`; flat beyond `TREE_INDENT_MAX_DEPTH`. */
export function treeIndentPx(depth: number): number {
  return Math.min(depth, TREE_INDENT_MAX_DEPTH) * 16 + 8
}

/**
 * One row of the fully-flattened tree. A single ordered array of these spans
 * every category section, every volume subtree and the trash row, so the whole
 * tree can be driven by one virtualizer (only the visible window mounts) rather
 * than a real `<TreeNode>` per row.
 */
export type TreeFlatRow =
  | { kind: 'header'; label: string; renderKey: string }
  | { kind: 'node'; path: string; depth: number; volume?: VolumeInfo; renderKey: string }
  | { kind: 'fold-siblings'; parentPath: string; count: number; depth: number; renderKey: string }
  | { kind: 'fold-levels'; hiddenPaths: string[]; depth: number; renderKey: string }
  | { kind: 'trash'; renderKey: string }

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
  if (row.kind === 'header') {
    return TREE_HEADER_HEIGHT_PX
  }
  if (row.kind === 'node' && row.volume && volumeUsedFraction(row.volume) !== null) {
    return TREE_DRIVE_ROW_HEIGHT_PX
  }
  return TREE_ROW_HEIGHT_PX
}

/**
 * Flatten the store's volumes + tree nodes into one ordered row array in render
 * order: a heading per non-empty category, then each volume's visible subtree
 * (root + expanded descendants via `flattenVisibleTree`, folded along the
 * active path), with the trash row appended after the `fixed` group.
 * Favourites live in their own panel above the tree, not in this model.
 */
export function buildTreeFlatModel(
  treeNodes: Record<string, TreeNodeState>,
  volumes: VolumeInfo[],
  fold: TreeFoldState = EMPTY_TREE_FOLD,
  os: PlatformOs = 'windows',
): TreeFlatModel {
  const groups = groupVolumesByCategory(volumes)
  const volumeRootKeys = new Set(volumes.map((volume) => volume.mountRoot.toLowerCase()))
  const rows: TreeFlatRow[] = []
  const indexByPath = new Map<string, number>()

  for (const group of groups) {
    // Finder calls fixed volumes "Locations"; Windows users expect "Drives".
    const label = group.category === 'fixed' && os === 'macos' ? 'Locations' : group.label
    rows.push({ kind: 'header', label, renderKey: `header-${group.category}` })
    for (const volume of group.volumes) {
      const nestedVolumeRoots = new Set(volumeRootKeys)
      nestedVolumeRoots.delete(volume.mountRoot.toLowerCase())

      for (const row of flattenVisibleTree(treeNodes, volume.mountRoot, nestedVolumeRoots, fold)) {
        if (row.kind === 'fold-siblings') {
          rows.push({ ...row, renderKey: `fold-siblings-${row.parentPath}` })
          continue
        }
        if (row.kind === 'fold-levels') {
          rows.push({ ...row, renderKey: `fold-levels-${volume.mountRoot}` })
          continue
        }
        indexByPath.set(row.path, rows.length)
        rows.push({
          kind: 'node',
          path: row.path,
          depth: row.depth,
          volume: row.depth === 0 ? volume : undefined,
          // Include the volume identity so keys stay stable even when two
          // platform roots use equivalent-looking path text.
          renderKey: `node-${group.category}-${volume.mountRoot}-${row.path}-${row.depth}`,
        })
      }
    }
    if (group.category === 'fixed') {
      indexByPath.set(TRASH_PATH, rows.length)
      rows.push({ kind: 'trash', renderKey: 'trash-fixed' })
    }
  }

  const offsets = new Array<number>(rows.length + 1)
  offsets[0] = 0
  for (let i = 0; i < rows.length; i += 1) {
    offsets[i + 1] = offsets[i] + treeRowHeight(rows[i])
  }

  return { rows, offsets, indexByPath }
}
