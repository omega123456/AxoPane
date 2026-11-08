import { create } from 'zustand'
import {
  cancelSizes,
  listTreeChildren,
  requestVisibleItemCounts as requestVisibleItemCountsCommand,
  sortActiveItems,
  listTrash,
  requestFolderSize,
  requestFolderSizes,
  requestIcons,
  saveSession,
  setSizeSubscriptions,
  setTabWatch,
} from '@/lib/ipc/commands'
import { isTrashPath, trashEntryToDirectoryEntry } from '@/lib/trash'
import { pageIndexForRow } from '@/stores/panes/entry-collection'
import type {
  DirectoryEntry,
  DirPatchEvent,
  FolderSizeIndexStatus,
  IconStateEvent,
  ItemCountEvent,
  SessionPane,
  SessionPatchEvent,
  SessionState,
  SessionViewParams,
  SizeStateEvent,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import {
  activeTab,
  fromSessionPane,
  toSessionPane,
  type TabMoveResult,
  useTabsStore,
} from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
import { log } from '@/lib/app-log-commands'
import { pathsMatch } from '@/lib/path-compare'
import { findVolumeForPath, isPathInsideVolume } from '@/lib/volumes'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { useSelectionStore } from '@/stores/selection-store'
import { ListingSession } from '@/stores/panes/listing-session'
import type { PaneId } from '@/types/pane'
import { cacheNow } from '@/stores/weighted-cache'
import {
  defaultPanesData,
  directoryPaths,
  type PaneState,
  type PanesData,
} from '@/stores/panes/pane-state'
import { getParentPath } from '@/stores/panes/pane-paths'
import {
  clientSortedEntries,
  sortedPanes,
  sortEntries,
  trashSortedPanes,
} from '@/stores/panes/entry-sort'
import {
  AUTO_FOLDER_SIZE_MAX_DIRECTORIES,
  appliedSizeStates,
  collectPendingSizeRequests,
  eagerSizesEnabled,
  refreshSizePaths,
  staleSizePathsOnLeave,
  summarizeSizeEvents,
  withoutStaleSizes,
  withPendingSizeRequests,
  type SizeEventBatchSummary,
} from '@/stores/panes/size-requests'
import {
  appliedIconStates,
  collectPendingIconRequests,
  withPendingIconRequests,
} from '@/stores/panes/icon-requests'
import {
  appliedItemCounts,
  canCountVisibleItems,
  canStartItemsSort,
  completedItemsSort,
  failedItemsSort,
  itemCountContext,
  startedItemsSort,
  supersededItemsSort,
  visibleItemCountPaths,
  withoutRequestedItemCounts,
  withRequestedItemCounts,
  type ItemCountScope,
  type ItemsSortOperation,
  type ItemsSortScope,
} from '@/stores/panes/item-counts'
import { dirPatchOutcome, paneIdFromTabId } from '@/stores/panes/dir-patch'
import {
  dirPatchFromSessionPatch,
  replacementViewApplied,
  sessionPaneId,
  type ReplacementView,
} from '@/stores/panes/session-patch'
import {
  DEFAULT_TAB_VIEW,
  directoryListingResolved,
  findRevealEntry,
  focusRequested,
  initializedPanes,
  listingFailed,
  listingStarted,
  listingViewOf,
  matchesListingView,
  navigatedPanes,
  paneResetForTab,
  revealTargetAfterNavigation,
  tabViewOf,
  trashListingResolved,
  withAppliedFilter,
  withClearedFilter,
  withFilterDraft,
  withFilterTimer,
  withFocusedEntry,
  withHistoryIndex,
  withScrollPosition,
  type ListingView,
  type SelectionSnapshot,
} from '@/stores/panes/listing-transitions'
import {
  buildRootTreeState,
  failedTreeNodes,
  loadedTreeNodes,
  prunedTreeNodes,
  revealChain,
  revealedTree,
  toggledTreeNode,
  withHiddenLevelsShown,
  withRevealedSiblings,
  withVolumes,
} from '@/stores/panes/tree-state'

export type { TreeFoldReveals, TreeNodeState } from '@/stores/panes/pane-state'
export { getParentPath } from '@/stores/panes/pane-paths'
export {
  AUTO_FOLDER_SIZE_MAX_DIRECTORIES,
  autoFolderSizeDisabledForPane,
  paneDirectoryCount,
} from '@/stores/panes/size-requests'

type NavigateOptions = {
  viaHistory?: boolean
  revealPath?: string
}

type InitializePayload = {
  session: SessionState
  showHiddenFiles: boolean
  folderSizeIndexStatus: FolderSizeIndexStatus
  volumes: VolumeInfo[]
}

type PanesStore = PanesData & {
  initialize: (payload: InitializePayload) => void
  setActivePane: (paneId: PaneId) => void
  reloadPane: (paneId: PaneId) => Promise<void>
  navigatePane: (paneId: PaneId, path: string, options?: NavigateOptions) => Promise<void>
  goUp: (paneId: PaneId) => Promise<void>
  goBack: (paneId: PaneId) => Promise<void>
  goForward: (paneId: PaneId) => Promise<void>
  openTabFromPath: (
    paneId: PaneId,
    path: string,
    options?: { revealPath?: string },
  ) => Promise<void>
  closeTab: (paneId: PaneId, tabId: string) => Promise<void>
  closeTabsBeside: (paneId: PaneId, tabId: string, side: 'left' | 'right') => Promise<void>
  setTabLocked: (paneId: PaneId, tabId: string, locked: boolean) => void
  switchTab: (paneId: PaneId, tabId: string) => Promise<void>
  moveTab: (
    sourcePaneId: PaneId,
    tabId: string,
    destinationPaneId: PaneId,
    destinationIndex: number,
  ) => Promise<TabMoveResult>
  refreshSizes: (paneId: PaneId) => Promise<void>
  finalizeListing: (paneId: PaneId) => Promise<void>
  applyDirPatch: (event: DirPatchEvent) => void
  applySessionPatch: (event: SessionPatchEvent) => void
  setSort: (paneId: PaneId, sortKey: SortKey) => Promise<void>
  setFilterDraft: (paneId: PaneId, value: string) => void
  clearFilter: (paneId: PaneId) => void
  setFocusedEntry: (paneId: PaneId, entryId: string | null) => void
  setScrollPosition: (paneId: PaneId, path: string, scrollTop: number) => void
  applySizeStates: (events: SizeStateEvent[]) => void
  requestVisibleItemCounts: (
    paneId: PaneId,
    visibleStartIndex: number,
    visibleEndIndex: number,
    viewportCount: number,
  ) => Promise<void>
  applyItemCountEvents: (events: ItemCountEvent[]) => void
  requestVisibleIcons: (paneId: PaneId, paths: string[]) => Promise<void>
  applyIconStates: (events: IconStateEvent[]) => void
  setFolderSizeIndexStatus: (status: FolderSizeIndexStatus) => void
  setVolumes: (volumes: VolumeInfo[]) => void
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  requestManualSize: (paneId: PaneId, entryId: string) => Promise<void>
  calculateAllFolderSizes: (paneId: PaneId) => Promise<void>
  ensureTreeChildren: (path: string | null) => Promise<void>
  revealPath: (path: string | null) => Promise<void>
  toggleTreeNode: (path: string) => Promise<void>
  revealTreeSiblings: (parentPath: string) => void
  revealTreeHiddenLevels: () => void
  reset: () => void
}

type ListingAttempt = {
  paneId: PaneId
  tabId: string
  listingGeneration: number
  requestedView: ListingView
}

const FILTER_DELAY_MILLISECONDS = 180
const STALE_SIZE_LOG_SAMPLE_LIMIT = 3
const SIZE_EVENT_LOG_INTERVAL = 2000
const SESSION_PERSIST_DELAY_MILLISECONDS = 200
const FIRST_PAGE_INDEX = 0
const WINDOWS_PLATFORM_PREFIX = 'Win'
const FAILED_RANGE_MESSAGE = 'Failed to load directory range'
const FAILED_LISTING_MESSAGE = 'Failed to load directory'
const DEFAULT_SESSION_PATH = '.'

const treeLoadFlights = new Map<string, Promise<void>>()
let treeLoadGeneration = 0

const pendingRevealPaths: Partial<Record<PaneId, string>> = {}
let revealScrollRequestCounter = 0

let sizeEventsProcessed = 0
let sessionPersistTimer: number | undefined
let listRequestIdCounter = 0

const sessionChangeWaiters: Record<PaneId, Set<() => void>> = { left: new Set(), right: new Set() }

function notifySessionChange(paneId: PaneId) {
  for (const waiter of sessionChangeWaiters[paneId]) {
    waiter()
  }
}

const listingSessions: Record<PaneId, ListingSession> = {
  left: new ListingSession('left', 'left', { onChange: () => notifySessionChange('left') }),
  right: new ListingSession('right', 'right', { onChange: () => notifySessionChange('right') }),
}
const replacementViewTokens: Record<PaneId, number> = { left: 0, right: 0 }

function nextListRequestId() {
  listRequestIdCounter += 1
  return listRequestIdCounter
}

function currentViewParams(pane: PaneState, showHiddenFiles: boolean): SessionViewParams {
  return {
    sortKey: pane.sortKey,
    sortDirection: pane.sortDirection,
    filter: pane.filterApplied,
    showHidden: showHiddenFiles,
    includeItemCounts: false,
  }
}

function waitForSessionChange(paneId: PaneId, check: () => boolean): Promise<void> {
  if (check()) {
    return Promise.resolve()
  }
  return new Promise<void>((resolve) => {
    const waiter = () => {
      if (!check()) {
        return
      }
      sessionChangeWaiters[paneId].delete(waiter)
      resolve()
    }
    sessionChangeWaiters[paneId].add(waiter)
  })
}

async function loadSessionPage(
  paneId: PaneId,
  session: ListingSession,
  pageIndex: number,
  stillCurrent: () => boolean,
) {
  const { collection } = session
  session.ensurePageLoaded(pageIndex)
  await waitForSessionChange(
    paneId,
    () =>
      !stillCurrent() || collection.isPageLoaded(pageIndex) || session.currentStatus === 'error',
  )
  if (session.currentStatus === 'error') {
    throw new Error(session.currentError ?? FAILED_RANGE_MESSAGE)
  }
}

async function ensureSessionFullyLoaded(paneId: PaneId, session: ListingSession): Promise<void> {
  const baselineAtStart = session.currentBaseline
  const { collection } = session
  if (collection.totalRows === 0 || !baselineAtStart) {
    return
  }

  const stillCurrent = () => session.currentBaseline === baselineAtStart
  const lastPageIndex = pageIndexForRow(collection.totalRows - 1)
  const allPageIndexes = Array.from({ length: lastPageIndex + 1 }, (_, index) => index)
  collection.setPinnedPages(allPageIndexes)

  for (const pageIndex of allPageIndexes) {
    if (!stillCurrent()) {
      return
    }
    if (collection.isPageLoaded(pageIndex)) {
      continue
    }
    await loadSessionPage(paneId, session, pageIndex, stillCurrent)
  }

  if (stillCurrent()) {
    session.pinViewportPages(FIRST_PAGE_INDEX)
  }
}

const entryIndexByArray = new WeakMap<DirectoryEntry[], Map<string, DirectoryEntry>>()

export function indexEntriesById(entries: DirectoryEntry[]): Map<string, DirectoryEntry> {
  let index = entryIndexByArray.get(entries)
  if (!index) {
    index = new Map(entries.map((entry) => [entry.id, entry]))
    entryIndexByArray.set(entries, index)
  }
  return index
}

function isWindowsNavigator() {
  return typeof navigator !== 'undefined' && navigator.platform.startsWith(WINDOWS_PLATFORM_PREFIX)
}

function logSizeEventProgress({
  batchSize,
  calculatingCount,
  terminalCount,
}: SizeEventBatchSummary) {
  const previousBucket = Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL)
  sizeEventsProcessed += batchSize
  const crossedLogInterval =
    Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL) > previousBucket
  if (!crossedLogInterval) {
    return
  }
  log.debug('size: events applied', {
    totalProcessed: sizeEventsProcessed,
    lastBatch: batchSize,
    calculatingInBatch: calculatingCount,
    terminalInBatch: terminalCount,
  })
}

async function reportSizeSubscriptions(
  paneId: PaneId,
  currentPath: string | null,
  folderPaths: string[],
) {
  try {
    await setSizeSubscriptions({ paneId, currentPath, folderPaths })
  } catch (error) {
    log.warn('size: setSizeSubscriptions failed', { paneId, error })
  }
}

function buildSessionState(activePaneId: PaneId): SessionState {
  return {
    activePane: activePaneId,
    leftPath: activeTab('left').path,
    rightPath: activeTab('right').path,
    left: toSessionPane('left'),
    right: toSessionPane('right'),
    recentFolders: useRecentFoldersStore.getState().folders,
  }
}

export function schedulePersistSession(activePaneId: PaneId) {
  if (typeof window === 'undefined') {
    return
  }

  if (sessionPersistTimer) {
    window.clearTimeout(sessionPersistTimer)
  }

  sessionPersistTimer = window.setTimeout(() => {
    void saveSession(buildSessionState(activePaneId))
  }, SESSION_PERSIST_DELAY_MILLISECONDS)
}

function sessionPaneOrDefault(pane: SessionPane | null | undefined, path: string): SessionPane {
  if (pane) {
    return pane
  }
  return {
    activeTabIndex: 0,
    tabs: [{ path: path || DEFAULT_SESSION_PATH, ...DEFAULT_TAB_VIEW, locked: false }],
  }
}

function parkRevealPath(paneId: PaneId, revealTarget: string | undefined) {
  if (revealTarget) {
    pendingRevealPaths[paneId] = revealTarget
    return
  }
  delete pendingRevealPaths[paneId]
}

function takeRevealPath(paneId: PaneId) {
  const revealPath = pendingRevealPaths[paneId]
  delete pendingRevealPaths[paneId]
  return revealPath
}

function selectionSnapshot(paneId: PaneId): SelectionSnapshot {
  const selection = useSelectionStore.getState().selections[paneId]
  return { focusedId: selection.focusedId, selectedIds: selection.selectedIds }
}

function isListingAttemptStale(attempt: ListingAttempt) {
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

function itemCountScope(paneId: PaneId, pane: PaneState): ItemCountScope {
  const activeTabId = activeTab(paneId).id
  return { paneId, activeTabId, context: itemCountContext(paneId, pane, activeTabId) }
}

function itemsSortScope(paneId: PaneId, pane: PaneState): ItemsSortScope {
  return {
    ...itemCountScope(paneId, pane),
    expected: { filterApplied: pane.filterApplied, sortDirection: pane.sortDirection },
  }
}

function withLiveActiveTab<Scope extends ItemCountScope>(scope: Scope): Scope {
  return { ...scope, activeTabId: activeTab(scope.paneId).id }
}

function activeTabIds() {
  return { left: activeTab('left').id, right: activeTab('right').id }
}

function setPanes(patch: Partial<PanesData> | null) {
  if (patch) {
    usePanesStore.setState(patch)
  }
}

async function runActiveItemsSort(scope: ItemsSortScope) {
  const pane = usePanesStore.getState().panes[scope.paneId]
  if (!canStartItemsSort(pane, scope)) {
    return
  }

  const started = startedItemsSort(usePanesStore.getState(), withLiveActiveTab(scope))
  if (!started) {
    return
  }
  usePanesStore.setState(started.next)
  const operation: ItemsSortOperation = { ...scope, operationId: started.operationId }

  try {
    const response = await sortActiveItems({
      context: scope.context,
      sortDirection: scope.expected.sortDirection,
      filter: scope.expected.filterApplied,
      showHidden: usePanesStore.getState().showHiddenFiles,
    })
    const liveOperation = withLiveActiveTab(operation)
    if (response.kind === 'superseded') {
      setPanes(supersededItemsSort(usePanesStore.getState(), liveOperation, response.context))
      return
    }
    setPanes(completedItemsSort(usePanesStore.getState(), liveOperation, response))
  } catch (error) {
    setPanes(failedItemsSort(usePanesStore.getState(), withLiveActiveTab(operation), error))
  }
}

function runActiveItemsSortForPane(paneId: PaneId) {
  const pane = usePanesStore.getState().panes[paneId]
  return runActiveItemsSort(itemsSortScope(paneId, pane))
}

function syncActiveTabWithPane(paneId: PaneId) {
  const pane = usePanesStore.getState().panes[paneId]
  useTabsStore.getState().patchActiveTab(paneId, tabViewOf(pane))
}

async function reloadTrashPane(paneId: PaneId, pane: PaneState, selection: SelectionSnapshot) {
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

function nextRevealScrollRequestId(revealEntry: DirectoryEntry | undefined) {
  if (revealEntry) {
    revealScrollRequestCounter += 1
  }
  return revealScrollRequestCounter
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

async function loadDirectoryListing(
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

function recordResolvedListing(paneId: PaneId) {
  syncActiveTabWithPane(paneId)
  useRecentFoldersStore.getState().record(usePanesStore.getState().panes[paneId].path)
  schedulePersistSession(usePanesStore.getState().activePaneId)
}

function cancelStaleSizes(paneId: PaneId, staleSizePaths: string[]) {
  usePanesStore.setState((state) => withoutStaleSizes(state, staleSizePaths))
  const cancelStartedAt = performance.now()
  void cancelSizes(staleSizePaths)
    .then((response) => {
      log.debug('size: navigate cancel result', {
        paneId,
        requested: staleSizePaths.length,
        cancelled: response.cancelled,
        elapsedMilliseconds: Math.round(performance.now() - cancelStartedAt),
      })
    })
    .catch((error) => {
      log.error('navigatePane failed to cancel folder sizes', { paneId, error })
    })
}

function releaseSizesOnLeave(paneId: PaneId, destinationPath: string) {
  const state = usePanesStore.getState()
  const leaving = state.panes[paneId]
  const staleSizePaths = staleSizePathsOnLeave(state, paneId)
  log.debug('size: navigate cancel evaluation', {
    paneId,
    from: leaving.path,
    to: destinationPath,
    leavingDirs: directoryPaths(leaving.entries).length,
    stale: staleSizePaths.length,
    sampleStale: staleSizePaths.slice(0, STALE_SIZE_LOG_SAMPLE_LIMIT),
  })
  if (staleSizePaths.length > 0) {
    cancelStaleSizes(paneId, staleSizePaths)
  }
}

function resetPaneForActiveTab(paneId: PaneId) {
  const tab = activeTab(paneId)
  usePanesStore.setState((state) => paneResetForTab(state, paneId, tab))
}

function clearFilterTimer(paneId: PaneId) {
  const timer = usePanesStore.getState().filterTimers[paneId]
  if (timer) {
    window.clearTimeout(timer)
  }
}

async function requestSizesMarkingPending(paths: string[], request: () => Promise<unknown>) {
  usePanesStore.setState((state) => withPendingSizeRequests(state, paths, true))
  try {
    await request()
  } catch (error) {
    usePanesStore.setState((state) => withPendingSizeRequests(state, paths, false))
    throw error
  }
}

function eagerSizingEnabledNow() {
  return eagerSizesEnabled(
    usePanesStore.getState().folderSizeIndexStatus,
    useConfigStore.getState().autoFolderSize,
  )
}

async function requestEagerDirectorySizes(paths: string[]) {
  if (!eagerSizingEnabledNow()) {
    return []
  }
  if (paths.length > AUTO_FOLDER_SIZE_MAX_DIRECTORIES) {
    return []
  }

  const pending = collectPendingSizeRequests(paths, usePanesStore.getState())
  if (pending.length === 0) {
    return []
  }
  await requestSizesMarkingPending(pending, () =>
    requestFolderSizes({ paths: pending, mode: 'auto' }),
  )
  return pending
}

async function armTabWatch(paneId: PaneId, pane: PaneState) {
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

function applyAcceptedDirPatch(event: DirPatchEvent) {
  const state = usePanesStore.getState()
  const paneId = paneIdFromTabId(event.tabId)
  const pane = state.panes[paneId]
  const outcome = dirPatchOutcome(state, paneId, event, activeTab(paneId).id)

  if (outcome.kind === 'skipped') {
    logSkippedDirPatch(outcome.reason, paneId, pane, event)
    return
  }
  usePanesStore.setState(outcome.next)
  if (outcome.kind === 'itemsSortPending') {
    queueMicrotask(() => void runActiveItemsSortForPane(paneId))
    return
  }
  log.debug('applyDirPatch applied', {
    paneId,
    path: event.path,
    entries: outcome.entryCount,
    changed: event.changed.length,
    removed: event.removed.length,
  })
}

const dirPatchSkipLoggers = {
  typing: (paneId: PaneId, _pane: PaneState, event: DirPatchEvent) =>
    log.debug('applyDirPatch skipped (typing)', { paneId, path: event.path }),
  inactiveTab: (paneId: PaneId, _pane: PaneState, event: DirPatchEvent) =>
    log.debug('applyDirPatch skipped (inactive tab)', { paneId, tabId: event.tabId }),
  pathMismatch: (paneId: PaneId, pane: PaneState, event: DirPatchEvent) =>
    log.debug('applyDirPatch skipped (path mismatch)', {
      paneId,
      panePath: pane.path,
      eventPath: event.path,
    }),
}

function logSkippedDirPatch(
  reason: keyof typeof dirPatchSkipLoggers,
  paneId: PaneId,
  pane: PaneState,
  event: DirPatchEvent,
) {
  dirPatchSkipLoggers[reason](paneId, pane, event)
}

function sessionStillShowsEvent(paneId: PaneId, event: SessionPatchEvent) {
  const sameTab = activeTab(paneId).id === event.tabId
  return sameTab && pathsMatch(usePanesStore.getState().panes[paneId].path, event.path)
}

type ReplaceViewEvent = Extract<SessionPatchEvent, { mode: 'replaceView' }>

async function settledReplacementBaseline(
  paneId: PaneId,
  event: ReplaceViewEvent,
  replacementToken: number,
) {
  const session = listingSessions[paneId]
  let acceptedBaseline = event.nextBaseline
  while (replacementViewTokens[paneId] === replacementToken) {
    await ensureSessionFullyLoaded(paneId, session)
    if (!sessionStillShowsEvent(paneId, event)) {
      return null
    }
    if (session.currentBaseline === acceptedBaseline) {
      return acceptedBaseline
    }
    if (!session.currentBaseline) {
      return null
    }
    acceptedBaseline = session.currentBaseline
  }
  return null
}

async function materializeReplacementView(paneId: PaneId, event: ReplaceViewEvent) {
  replacementViewTokens[paneId] += 1
  const replacementToken = replacementViewTokens[paneId]
  const session = listingSessions[paneId]
  const acceptedBaseline = await settledReplacementBaseline(paneId, event, replacementToken)
  const replacementSuperseded =
    acceptedBaseline === null || replacementViewTokens[paneId] !== replacementToken
  if (replacementSuperseded) {
    return
  }

  const replacement: ReplacementView = {
    paneId,
    tabId: event.tabId,
    path: event.path,
    loadedEntries: session.loadedEntries(),
    requestId: nextListRequestId(),
  }
  const stillAccepted = session.currentBaseline === acceptedBaseline
  if (stillAccepted && sessionStillShowsEvent(paneId, event)) {
    usePanesStore.setState((state) => replacementViewApplied(state, replacement))
  }
  void runActiveItemsSortForPane(paneId)
}

async function loadTreeChildren(path: string) {
  const currentNode = usePanesStore.getState().treeNodes[path]
  const requestGeneration = treeLoadGeneration
  const isSuperseded = () => requestGeneration !== treeLoadGeneration
  try {
    const response = await listTreeChildren({
      path,
      showHidden: usePanesStore.getState().showHiddenFiles,
    })
    if (!isSuperseded()) {
      usePanesStore.setState((state) => ({
        treeNodes: loadedTreeNodes(state, path, currentNode, response.children),
      }))
    }
  } catch {
    if (!isSuperseded()) {
      usePanesStore.setState((state) => ({
        treeNodes: failedTreeNodes(state, path, currentNode),
      }))
    }
  } finally {
    usePanesStore.setState((state) => ({
      treeNodes: prunedTreeNodes(state.treeNodes, state, state.treeRoots),
    }))
  }
}

async function loadTreeChildrenOnce(path: string) {
  let settleFlight: () => void = () => undefined
  const flight = new Promise<void>((resolve) => {
    settleFlight = resolve
  })
  treeLoadFlights.set(path, flight)
  try {
    await loadTreeChildren(path)
  } finally {
    treeLoadFlights.delete(path)
    settleFlight()
  }
}

export const usePanesStore = create<PanesStore>((set, get) => ({
  ...defaultPanesData(),
  initialize: ({ session, showHiddenFiles, folderSizeIndexStatus, volumes }) => {
    const tabsStore = useTabsStore.getState()
    tabsStore.hydrate(
      'left',
      fromSessionPane('left', sessionPaneOrDefault(session.left, session.leftPath)),
    )
    tabsStore.hydrate(
      'right',
      fromSessionPane('right', sessionPaneOrDefault(session.right, session.rightPath)),
    )
    useRecentFoldersStore.getState().hydrate(session.recentFolders ?? [])

    const initial = {
      activePaneId: session.activePane === 'right' ? ('right' as const) : ('left' as const),
      showHiddenFiles,
      folderSizeIndexStatus,
      volumes,
      leftTab: activeTab('left'),
      rightTab: activeTab('right'),
      ...buildRootTreeState(volumes),
    }
    set((state) => initializedPanes(state, initial))
  },
  setActivePane: (paneId) => {
    set({ activePaneId: paneId })
    schedulePersistSession(paneId)
  },
  reloadPane: async (paneId) => {
    const pane = get().panes[paneId]
    const revealChildPath = takeRevealPath(paneId)
    const selection = selectionSnapshot(paneId)
    set((state) => listingStarted(state, paneId))
    const attempt: ListingAttempt = {
      paneId,
      tabId: activeTab(paneId).id,
      listingGeneration: get().panes[paneId].listingGeneration,
      requestedView: listingViewOf(pane),
    }

    try {
      if (isTrashPath(pane.path)) {
        await reloadTrashPane(paneId, pane, selection)
        return
      }

      const resolved = await loadDirectoryListing(attempt, pane, selection, revealChildPath)
      if (!resolved) {
        return
      }
      recordResolvedListing(paneId)
      await runActiveItemsSortForPane(paneId)
      await get().finalizeListing(paneId)
    } catch (error) {
      if (isListingAttemptStale(attempt)) {
        return
      }
      log.error('reloadPane failed to load directory', { paneId, path: pane.path, error })
      set((state) => listingFailed(state, paneId, error))
    }
  },
  navigatePane: async (paneId, path, options = {}) => {
    log.debug('navigatePane', { paneId, path, viaHistory: options.viaHistory ?? false })
    const currentTab = activeTab(paneId)
    const lockedElsewhere = currentTab.locked && !pathsMatch(currentTab.path, path)
    if (lockedElsewhere) {
      await get().openTabFromPath(paneId, path, { revealPath: options.revealPath })
      return
    }
    const viaHistory = options.viaHistory ?? false
    if (!viaHistory) {
      clearFilterTimer(paneId)
    }

    const leavingPath = get().panes[paneId].path
    parkRevealPath(paneId, revealTargetAfterNavigation(leavingPath, path, options.revealPath))
    releaseSizesOnLeave(paneId, path)

    set((state) => navigatedPanes(state, paneId, path, viaHistory))
    await get().reloadPane(paneId)
  },
  goUp: async (paneId) => {
    const currentPath = get().panes[paneId].path
    const parentPath = getParentPath(currentPath)
    log.debug('goUp', { paneId, currentPath, parentPath })
    if (parentPath) {
      await get().navigatePane(paneId, parentPath)
    }
  },
  goBack: async (paneId) => {
    const pane = get().panes[paneId]
    if (pane.historyIndex <= 0) {
      return
    }
    await stepThroughHistory(paneId, pane.historyIndex - 1, 'goBack')
  },
  goForward: async (paneId) => {
    const pane = get().panes[paneId]
    const atHistoryEdge = pane.historyIndex < 0 || pane.historyIndex >= pane.history.length - 1
    if (atHistoryEdge) {
      return
    }
    await stepThroughHistory(paneId, pane.historyIndex + 1, 'goForward')
  },
  openTabFromPath: async (paneId, path, options = {}) => {
    log.debug('openTabFromPath', { paneId, path })
    parkRevealPath(paneId, options.revealPath)
    const tab = { path, ...DEFAULT_TAB_VIEW }
    useTabsStore.getState().addTab(paneId, { ...tab, locked: false })

    set((state) => ({ ...focusRequested(state, paneId), ...paneResetForTab(state, paneId, tab) }))

    void listingSessions[paneId].release()
    await get().reloadPane(paneId)
  },
  closeTab: async (paneId, tabId) => {
    const before = useTabsStore.getState().panes[paneId]
    const closingTab = before.tabs.find((tab) => tab.id === tabId)
    const cannotClose = before.tabs.length <= 1 || !closingTab || closingTab.locked
    if (cannotClose) {
      return
    }

    const wasActive = before.tabs[before.activeTabIndex]?.id === tabId
    useTabsStore.getState().closeTab(paneId, tabId)
    schedulePersistSession(get().activePaneId)

    if (!wasActive) {
      return
    }

    void reportSizeSubscriptions(paneId, null, [])
    resetPaneForActiveTab(paneId)
    void listingSessions[paneId].release()
    await get().reloadPane(paneId)
  },
  closeTabsBeside: async (paneId, tabId, side) => {
    const { tabs, activeTabIndex } = useTabsStore.getState().panes[paneId]
    const index = tabs.findIndex((tab) => tab.id === tabId)
    if (index === -1) {
      return
    }

    const besideTabs = side === 'left' ? tabs.slice(0, index) : tabs.slice(index + 1)
    const closing = besideTabs.filter((tab) => !tab.locked)
    const closesActiveTab = closing.some((tab) => tab.id === tabs[activeTabIndex]?.id)
    if (closesActiveTab) {
      await get().switchTab(paneId, tabId)
    }
    for (const tab of closing) {
      await get().closeTab(paneId, tab.id)
    }
  },
  setTabLocked: (paneId, tabId, locked) => {
    useTabsStore.getState().setTabLocked(paneId, tabId, locked)
    schedulePersistSession(get().activePaneId)
  },
  moveTab: async (sourcePaneId, tabId, destinationPaneId, destinationIndex) => {
    const result = useTabsStore
      .getState()
      .moveTab(sourcePaneId, tabId, destinationPaneId, destinationIndex)
    if (result.kind === 'none') {
      return result
    }

    schedulePersistSession(
      result.kind === 'transfer' ? result.destinationPaneId : get().activePaneId,
    )
    if (result.kind === 'reorder') {
      return result
    }

    const resetPaneIds = result.sourceActiveChanged
      ? [result.destinationPaneId, result.sourcePaneId]
      : [result.destinationPaneId]
    for (const paneId of resetPaneIds) {
      resetPaneForActiveTab(paneId)
    }
    set({ activePaneId: result.destinationPaneId })

    for (const paneId of resetPaneIds) {
      void listingSessions[paneId].release()
    }
    await Promise.all(resetPaneIds.map((paneId) => get().reloadPane(paneId)))
    return result
  },
  switchTab: async (paneId, tabId) => {
    const before = useTabsStore.getState().panes[paneId]
    if (before.tabs[before.activeTabIndex]?.id === tabId) {
      set({ activePaneId: paneId })
      return
    }

    useTabsStore.getState().setActiveTab(paneId, tabId)
    schedulePersistSession(paneId)

    const tab = activeTab(paneId)
    set((state) => ({ activePaneId: paneId, ...paneResetForTab(state, paneId, tab) }))
    void listingSessions[paneId].release()
    await get().reloadPane(paneId)
  },
  refreshSizes: async (paneId) => {
    await listingSessions[paneId].release()
    await get().reloadPane(paneId)

    const pane = get().panes[paneId]
    if (isTrashPath(pane.path)) {
      return
    }
    const folders = directoryPaths(pane.entries)
    const paths = refreshSizePaths(folders, get().sizeStates, eagerSizingEnabledNow())
    if (paths.length === 0) {
      return
    }
    await requestSizesMarkingPending(paths, () => requestFolderSizes({ paths, mode: 'refresh' }))
  },
  finalizeListing: async (paneId) => {
    const pane = get().panes[paneId]
    if (isTrashPath(pane.path)) {
      return
    }

    try {
      await armTabWatch(paneId, pane)
      const folders = directoryPaths(get().panes[paneId].entries)
      await reportSizeSubscriptions(
        paneId,
        pane.path,
        folders.slice(0, AUTO_FOLDER_SIZE_MAX_DIRECTORIES),
      )
      const requested = await requestEagerDirectorySizes(folders)

      log.debug('finalizeListing done', {
        paneId,
        path: pane.path,
        entries: get().panes[paneId].entries.length,
        sizeRequests: requested.length,
      })

      await get().ensureTreeChildren(getParentPath(pane.path) ?? pane.path)
    } catch (error) {
      log.error('finalizeListing failed', { paneId, path: pane.path, error })
    }
  },
  applyDirPatch: (event) => applyAcceptedDirPatch(event),
  applySessionPatch: (event) => {
    const paneId = sessionPaneId(event)
    if (!paneId || activeTab(paneId).id !== event.tabId) {
      return
    }
    const pathMatches = pathsMatch(get().panes[paneId].path, event.path)
    if (!pathMatches || !listingSessions[paneId].applyPatch(event)) {
      return
    }

    if (event.mode === 'replaceView') {
      void materializeReplacementView(paneId, event).catch((error) => {
        log.error('applySessionPatch failed to materialize replacement view', {
          paneId,
          path: event.path,
          error,
        })
      })
      return
    }
    get().applyDirPatch(dirPatchFromSessionPatch(event))
  },
  setSort: async (paneId, sortKey) => {
    const pane = get().panes[paneId]
    if (pane.loading || !pane.listingComplete) {
      return
    }
    if (isTrashPath(pane.path)) {
      set((state) => trashSortedPanes(state, paneId, sortKey))
      return
    }

    set((state) => sortedPanes(state, paneId, sortKey))
    const nextPane = get().panes[paneId]
    useTabsStore.getState().patchActiveTab(paneId, {
      sortKey: nextPane.sortKey,
      sortDirection: nextPane.sortDirection,
    })
    schedulePersistSession(get().activePaneId)
    if (sortKey === 'items') {
      await runActiveItemsSortForPane(paneId)
    }
  },
  setFilterDraft: (paneId, value) => {
    clearFilterTimer(paneId)
    set((state) => withFilterDraft(state, paneId, value))

    const nextTimer = window.setTimeout(() => {
      set((state) => withAppliedFilter(state, paneId, value))
      void get().reloadPane(paneId)
    }, FILTER_DELAY_MILLISECONDS)

    set((state) => withFilterTimer(state, paneId, nextTimer))
  },
  clearFilter: (paneId) => {
    clearFilterTimer(paneId)
    set((state) => withClearedFilter(state, paneId))
    void get().reloadPane(paneId)
  },
  setFocusedEntry: (paneId, entryId) => set((state) => withFocusedEntry(state, paneId, entryId)),
  setScrollPosition: (paneId, path, scrollTop) => {
    const previous = get().panes[paneId].scrollPositions[path]
    if (previous === scrollTop) {
      return
    }
    set((state) => withScrollPosition(state, paneId, path, scrollTop))
  },
  applySizeStates: (events) => {
    if (events.length === 0) {
      return
    }
    logSizeEventProgress(summarizeSizeEvents(events))
    set((state) => appliedSizeStates(state, events))
  },
  requestVisibleItemCounts: async (paneId, visibleStartIndex, visibleEndIndex, viewportCount) => {
    const pane = get().panes[paneId]
    if (!canCountVisibleItems(pane)) {
      return
    }

    const scope = itemCountScope(paneId, pane)
    const countWindow = { visibleStartIndex, visibleEndIndex, viewportCount }
    const pending = visibleItemCountPaths(
      pane,
      scope.context,
      countWindow,
      get().passiveItemCountRequests,
    )
    if (pending.length === 0) {
      return
    }

    setPanes(withRequestedItemCounts(get(), withLiveActiveTab(scope), pending))
    try {
      await requestVisibleItemCountsCommand({ context: scope.context, paths: pending })
    } catch (error) {
      set((state) => withoutRequestedItemCounts(state, scope.context, pending))
      throw error
    }
  },
  applyItemCountEvents: (events) => setPanes(appliedItemCounts(get(), events, activeTabIds())),
  requestVisibleIcons: async (paneId, paths) => {
    const pending = collectPendingIconRequests(get().panes[paneId].entries, paths, get())
    if (pending.length === 0) {
      return
    }

    set((state) => withPendingIconRequests(state, pending, true))
    try {
      await requestIcons({ paths: pending })
    } catch (error) {
      set((state) => withPendingIconRequests(state, pending, false))
      throw error
    }
  },
  applyIconStates: (events) => {
    const input = { events, now: cacheNow(), windowsPlatform: isWindowsNavigator() }
    set((state) => appliedIconStates(state, input))
  },
  setFolderSizeIndexStatus: (folderSizeIndexStatus) => set({ folderSizeIndexStatus }),
  setVolumes: (volumes) => {
    treeLoadGeneration += 1
    set((state) => withVolumes(state, volumes))
  },
  setShowHiddenFiles: async (showHiddenFiles) => {
    if (get().showHiddenFiles === showHiddenFiles) {
      return
    }
    set({ showHiddenFiles })
    await useConfigStore.getState().setShowHiddenFiles(showHiddenFiles)
    await Promise.all([get().reloadPane('left'), get().reloadPane('right')])
  },
  requestManualSize: async (paneId, entryId) => {
    const entry = get().panes[paneId].entries.find((item) => item.id === entryId)
    if (!entry?.isDir) {
      return
    }
    await requestFolderSize({ path: entry.path })
  },
  calculateAllFolderSizes: async (paneId) => {
    const pane = get().panes[paneId]
    const paths = directoryPaths(pane.entries)
    const pending = paths.filter((path) => !get().pendingSizeRequests[path])
    log.debug('size: calculateAllFolderSizes', {
      paneId,
      path: pane.path,
      directories: paths.length,
      pending: pending.length,
    })
    if (pending.length === 0) {
      return
    }

    const startedAt = performance.now()
    await requestSizesMarkingPending(pending, () => requestFolderSizes({ paths: pending }))
    log.debug('size: calculateAllFolderSizes request returned', {
      paneId,
      requested: pending.length,
      elapsedMilliseconds: Math.round(performance.now() - startedAt),
    })
  },
  ensureTreeChildren: async (path) => {
    if (!path) {
      return
    }
    if (get().treeNodes[path]?.loaded) {
      return
    }
    const existingFlight = treeLoadFlights.get(path)
    if (existingFlight) {
      return existingFlight
    }
    await loadTreeChildrenOnce(path)
  },
  revealPath: async (path) => {
    if (!path) {
      return
    }
    const root = findVolumeForPath(path, get().volumes)?.mountRoot ?? null
    if (!root) {
      return
    }

    const chain = revealChain(path, root)
    for (const ancestor of chain.slice(0, -1)) {
      await get().ensureTreeChildren(ancestor)
    }
    set((state) => revealedTree(state, path, chain))
  },
  toggleTreeNode: async (path) => {
    if (!get().treeNodes[path]?.loaded) {
      await get().ensureTreeChildren(path)
    }
    set((state) => toggledTreeNode(state, path))
  },
  revealTreeSiblings: (parentPath) =>
    set((state) => withRevealedSiblings(state, parentPath) ?? state),
  revealTreeHiddenLevels: () => set((state) => withHiddenLevelsShown(state)),
  reset: () => {
    treeLoadGeneration += 1
    treeLoadFlights.clear()
    delete pendingRevealPaths.left
    delete pendingRevealPaths.right
    useTabsStore.getState().reset()
    void listingSessions.left.release()
    void listingSessions.right.release()
    set(defaultPanesData())
  },
}))

async function stepThroughHistory(paneId: PaneId, historyIndex: number, action: string) {
  const store = usePanesStore.getState()
  const path = store.panes[paneId].history[historyIndex]
  log.debug(action, { paneId, path })
  const tab = activeTab(paneId)
  const lockedElsewhere = tab.locked && !pathsMatch(tab.path, path)
  if (lockedElsewhere) {
    await store.openTabFromPath(paneId, path)
    return
  }
  usePanesStore.setState((state) => withHistoryIndex(state, paneId, historyIndex))
  await store.navigatePane(paneId, path, { viaHistory: true })
}

export async function initializePanes() {
  const { ensureTreeChildren, treeRoots, panes, activePaneId } = usePanesStore.getState()
  const activePath = panes[activePaneId].path

  if (!activePath || isTrashPath(activePath)) {
    return
  }

  const activeRoot = treeRoots.find((root) => isPathInsideVolume(activePath, root))
  if (!activeRoot) {
    return
  }

  await ensureTreeChildren(activeRoot)
}
