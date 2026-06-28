import { buildAppContextMenuContent } from '@/lib/context-menu/app-menu-model'
import type {
  ContextMenuContent,
  ContextMenuNativeRequest,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import type { PlatformOs } from '@/lib/keymap'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { PaneId } from '@/types/pane'

const NATIVE_SECTION_ID = 'native-extras'

function fileChip(entry: DirectoryEntry): string {
  const dot = entry.name.lastIndexOf('.')
  const ext = dot > 0 ? entry.name.slice(dot + 1).toUpperCase() : ''
  return ext && ext.length <= 4 ? ext : 'FILE'
}

export function describeMenuTarget(target: ContextMenuTarget): { title: string; chip?: string } {
  switch (target.kind) {
    case 'file':
      return { title: target.entry.name, chip: fileChip(target.entry) }
    case 'folder':
      return { title: target.entry.name, chip: 'DIR' }
    case 'multi':
      return { title: 'Multiple items' }
    default:
      return { title: 'This folder' }
  }
}

export function buildContextMenuContent(
  paneId: PaneId,
  target: ContextMenuTarget,
  os: PlatformOs,
): ContextMenuContent {
  const content = buildAppContextMenuContent(paneId, target, os)
  const nativeRequest = os === 'windows' ? buildNativeMenuRequest(paneId, target) : null

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

function buildNativeMenuRequest(
  paneId: PaneId,
  target: ContextMenuTarget,
): ContextMenuNativeRequest | null {
  const pane = usePanesStore.getState().panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]

  switch (target.kind) {
    case 'file':
      return {
        targetKind: 'file',
        targetPath: target.entry.path,
        folderPath: pane.path,
        selectedPaths: [target.entry.path],
      }
    case 'folder':
      return {
        targetKind: 'folder',
        targetPath: target.entry.path,
        folderPath: pane.path,
        selectedPaths: [target.entry.path],
      }
    case 'multi': {
      const selectedEntries = pane.entries.filter((item) => selection.selectedIds.includes(item.id))
      const hasDirs = selectedEntries.some((item) => item.isDir)
      const hasFiles = selectedEntries.some((item) => !item.isDir)
      return {
        targetKind: hasDirs && hasFiles ? 'mixed' : 'multi',
        targetPath: null,
        folderPath: pane.path,
        selectedPaths: selectedEntries.map((item) => item.path),
      }
    }
    case 'empty':
      return {
        targetKind: 'background',
        targetPath: null,
        folderPath: pane.path,
        selectedPaths: [],
      }
    case 'tree': {
      const isDriveRoot = usePanesStore
        .getState()
        .volumes.some((volume) => volume.mountRoot.toLowerCase() === target.path.toLowerCase())
      return {
        targetKind: isDriveRoot ? 'driveRoot' : 'tree',
        targetPath: target.path,
        folderPath: target.path,
        selectedPaths: [target.path],
      }
    }
    case 'tab':
      return null
  }
}
