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

export const TREE_ROW_HEIGHT_PX = 28
export const TREE_DRIVE_ROW_HEIGHT_PX = 38
export const TREE_HEADER_HEIGHT_PX = 30

export const TREE_INDENT_MAX_DEPTH = 4
const TREE_INDENT_STEP_PX = 16
const TREE_ROW_BASE_PADDING_PX = 8

export function treeIndentPx(depth: number): number {
  return Math.min(depth, TREE_INDENT_MAX_DEPTH) * TREE_INDENT_STEP_PX + TREE_ROW_BASE_PADDING_PX
}

export type TreeFlatRow =
  | { kind: 'header'; label: string; renderKey: string }
  | { kind: 'node'; path: string; depth: number; volume?: VolumeInfo; renderKey: string }
  | { kind: 'fold-siblings'; parentPath: string; count: number; depth: number; renderKey: string }
  | { kind: 'fold-levels'; hiddenPaths: string[]; depth: number; renderKey: string }
  | { kind: 'trash'; renderKey: string }

export type TreeFlatModel = {
  rows: TreeFlatRow[]
  offsets: number[]
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
  return flattenVisibleTree(context.treeNodes, volume.mountRoot, {
    excludedRootPaths: nestedVolumeRoots,
    fold: context.fold,
  }).map((row) => toFlatRow(row, volume, category))
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

export type TreeFlatOptions = {
  fold?: TreeFoldState
  platform?: PlatformOs
}

export function buildTreeFlatModel(
  treeNodes: Record<string, TreeNodeState>,
  volumes: VolumeInfo[],
  options: TreeFlatOptions = {},
): TreeFlatModel {
  const { fold = EMPTY_TREE_FOLD, platform = 'windows' } = options
  const context: FlattenContext = {
    treeNodes,
    volumeRootKeys: new Set(volumes.map((volume) => volume.mountRoot.toLowerCase())),
    fold,
    platform,
  }
  const rows = groupVolumesByCategory(volumes).flatMap((group) => groupRows(group, context))
  return { rows, offsets: rowOffsets(rows), indexByPath: indexRowsByPath(rows) }
}
