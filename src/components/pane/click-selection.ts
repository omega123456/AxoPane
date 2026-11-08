import type { DirectoryEntry } from '@/lib/types/ipc'
import { hasPrimaryModifier } from '@/lib/key-table'

export type CurrentSelection = {
  selectedIds: string[]
  anchorId: string | null
}

export type ClickModifiers = {
  shiftKey: boolean
  ctrlKey: boolean
  metaKey: boolean
}

export type SelectionUpdate = {
  selectedIds: string[]
  anchorId: string | null
  focusedId: string
}

function entryIndex(entries: DirectoryEntry[], entryId: string | null) {
  return entries.findIndex((entry) => entry.id === entryId)
}

function rangeSelection(
  entries: DirectoryEntry[],
  anchorId: string,
  entryId: string,
): SelectionUpdate {
  const anchorIndex = entryIndex(entries, anchorId)
  const clickedIndex = entryIndex(entries, entryId)
  const start = Math.min(anchorIndex, clickedIndex)
  const end = Math.max(anchorIndex, clickedIndex)
  const rangeIds = entries.slice(start, end + 1).map((entry) => entry.id)
  return { selectedIds: rangeIds, anchorId, focusedId: entryId }
}

function toggledSelection(selection: CurrentSelection, entryId: string): SelectionUpdate {
  const currentIds = selection.selectedIds
  const selectedIds = currentIds.includes(entryId)
    ? currentIds.filter((selectedId) => selectedId !== entryId)
    : [...currentIds, entryId]
  return { selectedIds, anchorId: selection.anchorId ?? entryId, focusedId: entryId }
}

export function selectionForClick(
  entries: DirectoryEntry[],
  selection: CurrentSelection,
  entryId: string,
  modifiers: ClickModifiers,
): SelectionUpdate {
  const rangeAnchorId = modifiers.shiftKey ? selection.anchorId : null
  if (rangeAnchorId) {
    return rangeSelection(entries, rangeAnchorId, entryId)
  }
  const togglesEntry = hasPrimaryModifier(modifiers)
  if (togglesEntry) {
    return toggledSelection(selection, entryId)
  }
  return { selectedIds: [entryId], anchorId: entryId, focusedId: entryId }
}
