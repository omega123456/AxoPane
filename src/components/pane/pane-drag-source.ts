import type { DirectoryEntry } from '@/lib/types/ipc'
import type { DragItem } from '@/lib/drag-drop'

function toDragItem(entry: DirectoryEntry): DragItem {
  return {
    id: entry.id,
    name: entry.name,
    path: entry.path,
    isDir: entry.isDir,
    sizeBytes: entry.sizeBytes,
  }
}

function isTransferable(entry: DirectoryEntry) {
  return !entry.trashId
}

function draggedEntries(entries: DirectoryEntry[], selectedIds: string[], grabbed: DirectoryEntry) {
  if (!selectedIds.includes(grabbed.id)) {
    return [grabbed]
  }
  return entries.filter((entry) => selectedIds.includes(entry.id))
}

export function dragItemsForRow(
  entries: DirectoryEntry[],
  selectedIds: string[],
  entryId: string,
): DragItem[] {
  const grabbed = entries.find((entry) => entry.id === entryId)
  if (!grabbed || !isTransferable(grabbed)) {
    return []
  }
  return draggedEntries(entries, selectedIds, grabbed).filter(isTransferable).map(toDragItem)
}

export function directoryEntryById(entries: DirectoryEntry[], entryId: string) {
  const entry = entries.find((item) => item.id === entryId)
  return entry?.isDir ? entry : undefined
}
