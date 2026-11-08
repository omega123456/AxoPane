import { listTrash, setTabWatch } from '@/lib/ipc/commands'
import { pathsMatch } from '@/lib/path-compare'
import { trashEntryToDirectoryEntry } from '@/lib/trash'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import { sortEntries, clientSortedEntries } from '@/stores/panes/entry-sort'
import {
  directoryListingResolved,
  findRevealEntry,
  matchesListingView,
  paneResetForTab,
  tabViewOf,
  trashListingResolved,
  type ListingView,
  type SelectionSnapshot,
} from '@/stores/panes/listing-transitions'
import {
  currentViewParams,
  ensureSessionFullyLoaded,
  listingSessions,
  nextListRequestId,
  nextRevealScrollRequestId,
} from '@/stores/panes/listing-runtime'
import type { ListingSession } from '@/stores/panes/listing-session'
import type { PaneState } from '@/stores/panes/pane-state'
import { reportSizeSubscriptions } from '@/stores/panes/size-actions'
import { schedulePersistSession } from '@/stores/panes/session-persistence'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { useSelectionStore } from '@/stores/selection-store'
import { activeTab, useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

const FAILED_LISTING_MESSAGE = 'Failed to load directory'

export type ListingAttempt = {
  paneId: PaneId
  tabId: string
  listingGeneration: number
  requestedView: ListingView
}

export function selectionSnapshot(paneId: PaneId): SelectionSnapshot {
  const selection = useSelectionStore.getState().selections[paneId]
  return { focusedId: selection.focusedId, selectedIds: selection.selectedIds }
}

export function isListingAttemptStale(attempt: ListingAttempt) {
  const currentPane = usePanesStore.getState().panes[attempt.paneId]
  return (
    activeTab(attempt.paneId).id !== attempt.tabId ||
    currentPane.listingGeneration !== attempt.listingGeneration ||
    !matchesListingView(currentPane, attempt.requestedView)
  )
}

function isMaterializedListingStale(attempt: ListingAttempt, session: ListingSession) {
  return isListingAttemptStale(attempt) || session.currentBaseline == null
}

function revisesActiveSession(
  session: ListingSession,
  paneId: PaneId,
  tabId: string,
  path: string,
) {
  const sessionPath = session.currentPath
  if (session.currentBaseline == null) {
    return false
  }
  if (sessionPath == null) {
    return false
  }
  const sameTab = activeTab(paneId).id === tabId
  return sameTab && pathsMatch(sessionPath, path)
}

function throwIfSessionFailed(session: ListingSession) {
  if (session.currentStatus === 'error') {
    throw new Error(session.currentError ?? FAILED_LISTING_MESSAGE)
  }
}

export function syncActiveTabWithPane(paneId: PaneId) {
  const pane = usePanesStore.getState().panes[paneId]
  useTabsStore.getState().patchActiveTab(paneId, tabViewOf(pane))
}

export async function reloadTrashPane(
  paneId: PaneId,
  pane: PaneState,
  selection: SelectionSnapshot,
) {
  const response = await listTrash()
  const sortedEntries = sortEntries(
    response.entries.map(trashEntryToDirectoryEntry),
    pane.sortKey,
    pane.sortDirection,
    usePanesStore.getState().sizeStates,
  )

  const listing = { paneId, path: pane.path, sortedEntries, selection }
  usePanesStore.setState((state) => trashListingResolved(state, listing))

  syncActiveTabWithPane(paneId)
  schedulePersistSession(usePanesStore.getState().activePaneId)
  await reportSizeSubscriptions(paneId, null, [])
}

function selectRevealedEntry(paneId: PaneId, revealEntry: DirectoryEntry | undefined) {
  if (!revealEntry) {
    return
  }
  useSelectionStore
    .getState()
    .setSelection(paneId, [revealEntry.id], revealEntry.id, revealEntry.id)
}

async function openListingSession(paneId: PaneId, tabId: string, pane: PaneState) {
  const session = listingSessions[paneId]
  const view = currentViewParams(pane, usePanesStore.getState().showHiddenFiles)
  if (revisesActiveSession(session, paneId, tabId, pane.path)) {
    await session.reviseView(view)
    return session
  }
  await session.begin(tabId, pane.path, view)
  return session
}

export async function loadDirectoryListing(
  attempt: ListingAttempt,
  pane: PaneState,
  selection: SelectionSnapshot,
  revealChildPath: string | undefined,
) {
  const { paneId } = attempt
  const session = await openListingSession(paneId, attempt.tabId, pane)
  if (isListingAttemptStale(attempt)) {
    return false
  }
  throwIfSessionFailed(session)

  await ensureSessionFullyLoaded(paneId, session)
  if (isMaterializedListingStale(attempt, session)) {
    return false
  }

  const sortedEntries = clientSortedEntries(
    session.loadedEntries(),
    pane,
    usePanesStore.getState().sizeStates,
  )
  const revealEntry = findRevealEntry(sortedEntries, revealChildPath)
  const listing = {
    paneId,
    path: session.currentPath ?? pane.path,
    sortedEntries,
    selection,
    requestId: nextListRequestId(),
    revealEntry,
    revealScrollRequestId: nextRevealScrollRequestId(revealEntry),
  }
  usePanesStore.setState((state) => directoryListingResolved(state, listing))
  selectRevealedEntry(paneId, revealEntry)
  return true
}

export function recordResolvedListing(paneId: PaneId) {
  syncActiveTabWithPane(paneId)
  useRecentFoldersStore.getState().record(usePanesStore.getState().panes[paneId].path)
  schedulePersistSession(usePanesStore.getState().activePaneId)
}

export function resetPaneForActiveTab(paneId: PaneId) {
  const tab = activeTab(paneId)
  usePanesStore.setState((state) => paneResetForTab(state, paneId, tab))
}

export function clearFilterTimer(paneId: PaneId) {
  const timer = usePanesStore.getState().filterTimers[paneId]
  if (timer) {
    window.clearTimeout(timer)
  }
}

export async function armTabWatch(paneId: PaneId, pane: PaneState) {
  const tabId = activeTab(paneId).id
  await setTabWatch(
    {
      tabId,
      path: pane.path,
      sortKey: pane.sortKey,
      sortDirection: pane.sortDirection,
      filter: pane.filterApplied,
      showHidden: usePanesStore.getState().showHiddenFiles,
      includeItemCounts: false,
    },
    undefined,
    { tabId, requestId: pane.listRequestId, path: pane.path },
  )
}
