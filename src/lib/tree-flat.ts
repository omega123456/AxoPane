import type { VolumeGroup, VolumeInfo } from '@/lib/types/ipc'
import type { PlatformOs } from '@/lib/keymap'
import type { TreeNodeState } from '@/stores/panes-store'
import { TRASH_PATH } from '@/lib/trash'
import {
  EMPTY_TREE_FOLD,
  flattenVisibleTree,
  type FlatTreeRow,
  type TreeFoldState,
} from '@/lib/tree-sticky'
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
const TREE_INDENT_STEP_PX = 16
const TREE_ROW_BASE_PADDING_PX = 8

/** Left padding of a tree row at `depth`; flat beyond `TREE_INDENT_MAX_DEPTH`. */
export function treeIndentPx(depth: number): number {
  return Math.min(depth, TREE_INDENT_MAX_DEPTH) * TREE_INDENT_STEP_PX + TREE_ROW_BASE_PADDING_PX
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

function hasCapacityBar(row: TreeFlatRow) {
  if (row.kind !== 'node') {
    return false
  }
  if (!row.volume) {
    return false
  }
  return volumeUsedFraction(row.volume) !== null
}

/** Height in pixels of a single flat row, keyed on its kind. */
export function treeRowHeight(row: TreeFlatRow): number {
  if (row.kind === 'header') {
    return TREE_HEADER_HEIGHT_PX
  }
  if (hasCapacityBar(row)) {
    return TREE_DRIVE_ROW_HEIGHT_PX
  }
  return TREE_ROW_HEIGHT_PX
}

function groupHeading(group: VolumeGroup, platform: PlatformOs) {
  // Finder calls fixed volumes "Locations"; Windows users expect "Drives".
  const isMacLocations = group.category === 'fixed' && platform === 'macos'
  return isMacLocations ? 'Locations' : group.label
}

function toFlatRow(row: FlatTreeRow, volume: VolumeInfo, category: string): TreeFlatRow {
  if (row.kind === 'fold-siblings') {
    return { ...row, renderKey: `fold-siblings-${row.parentPath}` }
  }
  if (row.kind === 'fold-levels') {
    return { ...row, renderKey: `fold-levels-${volume.mountRoot}` }
  }
  return {
    kind: 'node',
    path: row.path,
    depth: row.depth,
    volume: row.depth === 0 ? volume : undefined,
    // Include the volume identity so keys stay stable even when two platform
    // roots use equivalent-looking path text.
    renderKey: `node-${category}-${volume.mountRoot}-${row.path}-${row.depth}`,
  }
}

type FlattenContext = {
  treeNodes: Record<string, TreeNodeState>
  volumeRootKeys: ReadonlySet<string>
  fold: TreeFoldState
  platform: PlatformOs
}

function volumeRows(volume: VolumeInfo, category: string, context: FlattenContext) {
  const nestedVolumeRoots = new Set(context.volumeRootKeys)
  nestedVolumeRoots.delete(volume.mountRoot.toLowerCase())
  return flattenVisibleTree(
    context.treeNodes,
    volume.mountRoot,
    nestedVolumeRoots,
    context.fold,
  ).map((row) => toFlatRow(row, volume, category))
}

function groupRows(group: VolumeGroup, context: FlattenContext): TreeFlatRow[] {
  const trashRow: TreeFlatRow[] =
    group.category === 'fixed' ? [{ kind: 'trash', renderKey: 'trash-fixed' }] : []
  return [
    {
      kind: 'header',
      label: groupHeading(group, context.platform),
      renderKey: `header-${group.category}`,
    },
    ...group.volumes.flatMap((volume) => volumeRows(volume, group.category, context)),
    ...trashRow,
  ]
}

function navigablePath(row: TreeFlatRow): string | undefined {
  if (row.kind === 'node') {
    return row.path
  }
  return row.kind === 'trash' ? TRASH_PATH : undefined
}

function indexRowsByPath(rows: TreeFlatRow[]) {
  const indexByPath = new Map<string, number>()
  rows.forEach((row, rowIndex) => {
    const path = navigablePath(row)
    if (path !== undefined) {
      indexByPath.set(path, rowIndex)
    }
  })
  return indexByPath
}

function rowOffsets(rows: TreeFlatRow[]) {
  const offsets = new Array<number>(rows.length + 1)
  offsets[0] = 0
  for (let rowIndex = 0; rowIndex < rows.length; rowIndex += 1) {
    offsets[rowIndex + 1] = offsets[rowIndex] + treeRowHeight(rows[rowIndex])
  }
  return offsets
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
  platform: PlatformOs = 'windows',
): TreeFlatModel {
  const context: FlattenContext = {
    treeNodes,
    volumeRootKeys: new Set(volumes.map((volume) => volume.mountRoot.toLowerCase())),
    fold,
    platform,
  }
  const rows = groupVolumesByCategory(volumes).flatMap((group) => groupRows(group, context))
  return { rows, offsets: rowOffsets(rows), indexByPath: indexRowsByPath(rows) }
}
