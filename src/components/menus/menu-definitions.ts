import { buildAppContextMenuContent } from '@/lib/context-menu/app-menu-model'
import type {
  ContextMenuContent,
  ContextMenuNativeRequest,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import type { PlatformOs } from '@/lib/keymap'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { isTrashPath } from '@/lib/trash'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { PaneId } from '@/types/pane'
import { dispatchByKind, type KindHandlers } from '@/lib/kind-table'

const NATIVE_SECTION_ID = 'native-extras'
const MAX_FILE_CHIP_LENGTH = 4

function fileChip(entry: DirectoryEntry): string {
  const extensionStart = entry.name.lastIndexOf('.')
  const extension = extensionStart > 0 ? entry.name.slice(extensionStart + 1).toUpperCase() : ''
  return extension && extension.length <= MAX_FILE_CHIP_LENGTH ? extension : 'FILE'
}

type MenuTargetDescription = { title: string; chip?: string }

const THIS_FOLDER_DESCRIPTION: MenuTargetDescription = { title: 'This folder' }

const menuTargetDescriptions: KindHandlers<ContextMenuTarget, [], MenuTargetDescription> = {
  file: (target) => ({ title: target.entry.name, chip: fileChip(target.entry) }),
  folder: (target) => ({ title: target.entry.name, chip: 'DIR' }),
  multi: () => ({ title: 'Multiple items' }),
  empty: () => THIS_FOLDER_DESCRIPTION,
  tab: () => THIS_FOLDER_DESCRIPTION,
  tree: () => THIS_FOLDER_DESCRIPTION,
  favourite: () => THIS_FOLDER_DESCRIPTION,
}

export function describeMenuTarget(target: ContextMenuTarget): MenuTargetDescription {
  return dispatchByKind(menuTargetDescriptions, target)
}

export function buildContextMenuContent(
  paneId: PaneId,
  target: ContextMenuTarget,
  platform: PlatformOs,
): ContextMenuContent {
  const content = buildAppContextMenuContent(paneId, target, platform)
  const nativeRequest = platform === 'windows' ? buildNativeMenuRequest(paneId, target) : null

  return {
    ...content,
    nativeRequest,
    nativeSectionId: nativeRequest ? NATIVE_SECTION_ID : null,
  }
}

export function resolveMenuTarget(paneId: PaneId, entry?: DirectoryEntry): ContextMenuTarget {
  const selection = useSelectionStore.getState().selections[paneId]

  if (!entry && selection.selectedIds.length > 1) {
    return { kind: 'multi' }
  }

  if (!entry) {
    return selection.selectedIds.length > 1 ? { kind: 'multi' } : { kind: 'empty' }
  }

  const multipleSelected =
    selection.selectedIds.length > 1 && selection.selectedIds.includes(entry.id)
  if (multipleSelected) {
    return { kind: 'multi' }
  }

  if (entry.isDir) {
    return { kind: 'folder', entry }
  }

  return { kind: 'file', entry }
}

type PaneSnapshot = ReturnType<typeof usePanesStore.getState>['panes'][PaneId]

function singleEntryRequest(
  targetKind: 'file' | 'folder',
  entry: DirectoryEntry,
  pane: PaneSnapshot,
): ContextMenuNativeRequest {
  return { targetKind, targetPath: entry.path, folderPath: pane.path, selectedPaths: [entry.path] }
}

function folderPathRequest(
  targetKind: 'tree' | 'driveRoot',
  path: string,
): ContextMenuNativeRequest {
  return { targetKind, targetPath: path, folderPath: path, selectedPaths: [path] }
}

function multiSelectionRequest(paneId: PaneId, pane: PaneSnapshot): ContextMenuNativeRequest {
  const selectedIdSet = new Set(useSelectionStore.getState().selections[paneId].selectedIds)
  const selectedEntries = pane.entries.filter((item) => selectedIdSet.has(item.id))
  const hasDirs = selectedEntries.some((item) => item.isDir)
  const hasFiles = selectedEntries.some((item) => !item.isDir)
  const mixesFoldersAndFiles = hasDirs && hasFiles
  return {
    targetKind: mixesFoldersAndFiles ? 'mixed' : 'multi',
    targetPath: null,
    folderPath: pane.path,
    selectedPaths: selectedEntries.map((item) => item.path),
  }
}

function isDriveRoot(path: string) {
  const lowerPath = path.toLowerCase()
  return usePanesStore
    .getState()
    .volumes.some((volume) => volume.mountRoot.toLowerCase() === lowerPath)
}

const nativeMenuRequests: KindHandlers<
  ContextMenuTarget,
  [paneId: PaneId, pane: PaneSnapshot],
  ContextMenuNativeRequest | null
> = {
  file: (target, _paneId, pane) => singleEntryRequest('file', target.entry, pane),
  folder: (target, _paneId, pane) => singleEntryRequest('folder', target.entry, pane),
  multi: (_target, paneId, pane) => multiSelectionRequest(paneId, pane),
  empty: (_target, _paneId, pane) => ({
    targetKind: 'background',
    targetPath: null,
    folderPath: pane.path,
    selectedPaths: [],
  }),
  tree: (target) => folderPathRequest(isDriveRoot(target.path) ? 'driveRoot' : 'tree', target.path),
  favourite: (target) => folderPathRequest('tree', target.path),
  tab: () => null,
}

// Trash entries have no real filesystem path (their `path` is an opaque
// trash id), and the trash pane's own path isn't a real folder either, so
// the native OS shell menu has nothing valid to operate on here.
function targetsTrash(pane: PaneSnapshot, target: ContextMenuTarget) {
  if (isTrashPath(pane.path)) {
    return true
  }
  return target.kind === 'tree' && isTrashPath(target.path)
}

function buildNativeMenuRequest(
  paneId: PaneId,
  target: ContextMenuTarget,
): ContextMenuNativeRequest | null {
  const pane = usePanesStore.getState().panes[paneId]
  if (targetsTrash(pane, target)) {
    return null
  }
  return dispatchByKind(nativeMenuRequests, target, paneId, pane)
}
