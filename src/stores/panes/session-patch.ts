import type {
  DirectoryEntry,
  DirPatchEvent,
  SessionPatchEvent,
  SessionRowDelta,
} from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { pathKey } from '@/lib/path-compare'
import { sortEntries, sortSpecFor } from '@/stores/panes/entry-sort'
import { withResolvedIcons } from '@/stores/panes/icon-requests'
import { clearPassiveItemCountRequestsForPane } from '@/stores/panes/item-counts'
import {
  retainedFocusId,
  withPane,
  type PaneState,
  type PanesData,
} from '@/stores/panes/pane-state'
import { applyTreeDirPatch } from '@/stores/panes/tree-dir-patch'
import { treeNodesUpdate } from '@/stores/panes/dir-patch'
import type { TabPath } from '@/stores/tabs-store'

type DirPatchChange = DirPatchEvent['changed'][number]

type EntryDelta = Extract<SessionRowDelta, { entry: DirectoryEntry }>

export type ReplacementView = {
  paneId: PaneId
  tabId: string
  path: string
  loadedEntries: DirectoryEntry[]
  requestId: number
}

const PANE_IDS: ReadonlySet<string> = new Set<PaneId>(['left', 'right'])

export function sessionPaneId(event: SessionPatchEvent): PaneId | null {
  return PANE_IDS.has(event.paneId) ? (event.paneId as PaneId) : null
}

function hasEntry(delta: SessionRowDelta): delta is EntryDelta {
  return 'entry' in delta
}

function changedRows(event: SessionPatchEvent): DirPatchChange[] {
  if (event.mode === 'metadataOnly') {
    return event.updates.map((update) => ({ path: update.path, entry: update.entry }))
  }
  if (event.mode === 'delta') {
    return event.deltas
      .filter(hasEntry)
      .map((delta) => ({ path: delta.entry.path, entry: delta.entry }))
  }
  return []
}

function removedRows(event: SessionPatchEvent): string[] {
  if (event.mode !== 'delta') {
    return []
  }
  return event.deltas.filter((delta) => delta.kind === 'removed').map((delta) => delta.path)
}

export function dirPatchFromSessionPatch(event: SessionPatchEvent): DirPatchEvent {
  return {
    tabId: event.tabId,
    path: event.path,
    reason: 'watch',
    changed: changedRows(event),
    removed: removedRows(event),
  }
}

function folderChangesForTree(
  { tabId, path }: TabPath,
  previous: PaneState,
  entries: DirectoryEntry[],
): DirPatchEvent {
  const entryPaths = new Set(entries.map((entry) => pathKey(entry.path)))
  return {
    tabId,
    path,
    reason: 'watch',
    changed: entries.filter((entry) => entry.isDir).map((entry) => ({ path: entry.path, entry })),
    removed: previous.entries
      .filter((entry) => entry.isDir && !entryPaths.has(pathKey(entry.path)))
      .map((entry) => entry.path),
  }
}

export function replacementViewApplied(
  state: PanesData,
  replacement: ReplacementView,
): Partial<PanesData> {
  const { paneId } = replacement
  const pane = state.panes[paneId]
  const ordered =
    pane.sortKey === 'size'
      ? sortEntries(replacement.loadedEntries, sortSpecFor(pane, state.sizeStates))
      : replacement.loadedEntries
  const entries = withResolvedIcons(ordered, state.resolvedIconPaths)
  const treeEvent = folderChangesForTree(replacement, pane, entries)

  return {
    passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
      state.passiveItemCountRequests,
      paneId,
    ),
    ...withPane(state, paneId, {
      entries,
      listRequestId: replacement.requestId,
      focusedEntryId: retainedFocusId(entries, pane.focusedEntryId),
    }),
    ...treeNodesUpdate(applyTreeDirPatch(state, treeEvent)),
  }
}
