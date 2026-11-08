import type { DirectoryEntry, DirPatchEvent, SortKey } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { pathsMatch } from '@/lib/path-compare'
import { insertSorted, sortEntries } from '@/stores/panes/entry-sort'
import { hydrateEntryIcon } from '@/stores/panes/icon-requests'
import { itemCountContext, withoutPatchedItemCountRequests } from '@/stores/panes/item-counts'
import {
  retainedFocusId,
  withPane,
  type PaneState,
  type PanesData,
  type SizeStates,
  type TreeNodes,
} from '@/stores/panes/pane-state'
import { applyTreeDirPatch } from '@/stores/panes/tree-state'

export type DirPatchSkipReason = 'typing' | 'inactiveTab' | 'pathMismatch'

export type DirPatchOutcome =
  | { kind: 'skipped'; reason: DirPatchSkipReason }
  | { kind: 'itemsSortPending'; next: Partial<PanesData> }
  | { kind: 'applied'; next: Partial<PanesData>; entryCount: number }

type PatchMerge = (
  kept: DirectoryEntry[],
  inserted: DirectoryEntry[],
  pane: PaneState,
  sizeStates: SizeStates,
) => DirectoryEntry[]

const RIGHT_PANE_TAB_PREFIX = 'right'

export function paneIdFromTabId(tabId: string): PaneId {
  return tabId.startsWith(RIGHT_PANE_TAB_PREFIX) ? 'right' : 'left'
}

function affectedPatchPaths(event: DirPatchEvent) {
  const affected = [...event.removed]
  for (const change of event.changed) {
    if (!affected.some((path) => pathsMatch(path, change.path))) {
      affected.push(change.path)
    }
  }
  return affected
}

function entryMatchesPane(entry: DirectoryEntry, pane: PaneState, showHiddenFiles: boolean) {
  const hiddenFromPane = !showHiddenFiles && (entry.isHidden || entry.isSystem)
  if (hiddenFromPane) {
    return false
  }
  return (
    pane.filterApplied === '' || entry.name.toLowerCase().includes(pane.filterApplied.toLowerCase())
  )
}

function patchInserts(event: DirPatchEvent, pane: PaneState, showHiddenFiles: boolean) {
  const inserts = new Map<string, DirectoryEntry>()
  for (const change of event.changed) {
    const matchingKey = Array.from(inserts.keys()).find((path) => pathsMatch(path, change.path))
    if (matchingKey) inserts.delete(matchingKey)
    if (change.entry && entryMatchesPane(change.entry, pane, showHiddenFiles)) {
      inserts.set(change.entry.path, change.entry)
    }
  }
  return Array.from(inserts.values())
}

const insertIntoSortedEntries: PatchMerge = (kept, inserted, pane, sizeStates) => {
  for (const entry of inserted) {
    insertSorted(kept, entry, pane.sortKey, pane.sortDirection, sizeStates)
  }
  return kept
}

const patchMergeBySortKey: Partial<Record<SortKey, PatchMerge>> = {
  size: (kept, inserted, pane, sizeStates) =>
    sortEntries([...kept, ...inserted], pane.sortKey, pane.sortDirection, sizeStates),
  items: (kept, inserted) => [...kept, ...inserted],
}

export function patchEntries(
  pane: PaneState,
  event: DirPatchEvent,
  state: Pick<PanesData, 'sizeStates' | 'showHiddenFiles' | 'resolvedIconPaths'>,
): DirectoryEntry[] | null {
  if (pane.path !== event.path) {
    return null
  }

  const affected = affectedPatchPaths(event)
  const kept =
    affected.length > 0
      ? pane.entries.filter((entry) => !affected.some((path) => pathsMatch(path, entry.path)))
      : [...pane.entries]
  const inserted = patchInserts(event, pane, state.showHiddenFiles).map((entry) =>
    hydrateEntryIcon(entry, state.resolvedIconPaths),
  )
  const merge = patchMergeBySortKey[pane.sortKey] ?? insertIntoSortedEntries
  return merge(kept, inserted, pane, state.sizeStates)
}

export function treeNodesUpdate(treeNodes: TreeNodes | null) {
  return treeNodes ? { treeNodes } : {}
}

function itemsSortedPatchOutcome(
  state: PanesData,
  paneId: PaneId,
  event: DirPatchEvent,
): DirPatchOutcome {
  const pane = state.panes[paneId]
  if (!pathsMatch(pane.path, event.path)) {
    return { kind: 'skipped', reason: 'pathMismatch' }
  }
  return {
    kind: 'itemsSortPending',
    next: {
      ...withPane(state, paneId, { itemsSortStatus: 'counting' }),
      ...treeNodesUpdate(applyTreeDirPatch(state, event)),
    },
  }
}

export function dirPatchOutcome(
  state: PanesData,
  paneId: PaneId,
  event: DirPatchEvent,
  activeTabId: string,
): DirPatchOutcome {
  const pane = state.panes[paneId]
  if (pane.typing) {
    return { kind: 'skipped', reason: 'typing' }
  }
  if (event.tabId !== activeTabId) {
    return { kind: 'skipped', reason: 'inactiveTab' }
  }
  if (pane.sortKey === 'items') {
    return itemsSortedPatchOutcome(state, paneId, event)
  }

  const entries = patchEntries(pane, event, state)
  if (!entries) {
    return { kind: 'skipped', reason: 'pathMismatch' }
  }

  const context = itemCountContext(paneId, pane, activeTabId)
  return {
    kind: 'applied',
    entryCount: entries.length,
    next: {
      passiveItemCountRequests: withoutPatchedItemCountRequests(state, context, event),
      ...withPane(state, paneId, {
        entries,
        focusedEntryId: retainedFocusId(entries, pane.focusedEntryId),
      }),
      ...treeNodesUpdate(applyTreeDirPatch(state, event)),
    },
  }
}
