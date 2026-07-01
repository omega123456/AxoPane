import type { DirectoryEntry, TrashEntry } from '@/lib/types/ipc'

/**
 * Virtual, non-filesystem path used to navigate a pane to the trash browser.
 * Deliberately contains no `/` or `\` so path-parsing helpers written for
 * real filesystem paths (e.g. `getParentPath`) treat it as rootless rather
 * than misparsing a segment out of it.
 */
export const TRASH_PATH = 'axopane:trash'

export function isTrashPath(path: string): boolean {
  return path === TRASH_PATH
}

export function trashEntryToDirectoryEntry(entry: TrashEntry): DirectoryEntry {
  return {
    id: entry.id,
    name: entry.name,
    path: entry.id,
    isDir: entry.isDir,
    iconDataUrl: null,
    sizeBytes: entry.sizeBytes,
    itemCount: null,
    typeLabel: entry.isDir ? 'Folder' : 'File',
    modifiedAt: entry.deletedAt === null ? null : new Date(entry.deletedAt * 1000).toISOString(),
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
    trashId: entry.id,
    originalPath: entry.originalPath,
  }
}
