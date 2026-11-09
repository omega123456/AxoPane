import { log } from '@/lib/app-log-commands'
import { pathsMatch } from '@/lib/path-compare'
import { isTrashPath } from '@/lib/trash'
import type { SortKey } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { panesStore } from '@/stores/panes/store-handle'
import { sortedPanes, trashSortedPanes } from '@/stores/panes/entry-sort'
import { runActiveItemsSortForPane } from '@/stores/panes/item-count-actions'
import {
  armTabWatch,
  clearFilterTimer,
  isListingAttemptStale,
  loadDirectoryListing,
  recordResolvedListing,
  reloadTrashPane,
  selectionSnapshot,
  type ListingAttempt,
} from '@/stores/panes/listing-load'
import {
  clearRevealPaths,
  listingSessions,
  parkRevealPath,
  takeRevealPath,
} from '@/stores/panes/listing-runtime'
import {
  initializedPanes,
  listingFailed,
  listingStarted,
  listingViewOf,
  navigatedPanes,
  revealTargetAfterNavigation,
  withAppliedFilter,
  withClearedFilter,
  withFilterDraft,
  withFilterTimer,
  withFocusedEntry,
  withHistoryIndex,
  withScrollPosition,
} from '@/stores/panes/listing-transitions'
import { getParentPath } from '@/stores/panes/pane-paths'
import { defaultPanesData, directoryPaths, isListingSettled } from '@/stores/panes/pane-state'
import { schedulePersistSession, sessionPaneOrDefault } from '@/stores/panes/session-persistence'
import {
  releaseSizesOnLeave,
  reportSizeSubscriptions,
  requestEagerDirectorySizes,
} from '@/stores/panes/size-actions'
import type { InitializePayload, NavigateOptions } from '@/stores/panes/store-types'
import { treeLoads } from '@/stores/panes/tree-actions'
import { buildRootTreeState } from '@/stores/panes/tree-state'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { activeTab, fromSessionPane, useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'
import { AUTO_FOLDER_SIZE_MAX_DIRECTORIES } from '@/stores/panes/size-requests'

const FILTER_DELAY_MILLISECONDS = 180

export function initialize({
  session,
  showHiddenFiles,
  folderSizeIndexStatus,
  volumes,
}: InitializePayload) {
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
  panesStore().setState((state) => initializedPanes(state, initial))
}

export function setActivePane(paneId: PaneId) {
  panesStore().setState({ activePaneId: paneId })
  schedulePersistSession(paneId)
}

export async function reloadPane(paneId: PaneId) {
  const pane = panesStore().getState().panes[paneId]
  const revealChildPath = takeRevealPath(paneId)
  const selection = selectionSnapshot(paneId)
  panesStore().setState((state) => listingStarted(state, paneId))
  const attempt: ListingAttempt = {
    paneId,
    tabId: activeTab(paneId).id,
    listingGeneration: panesStore().getState().panes[paneId].listingGeneration,
    requestedView: listingViewOf(pane),
  }

  try {
    if (isTrashPath(pane.path)) {
      await reloadTrashPane(paneId, pane, selection)
      return
    }

    const resolved = await loadDirectoryListing(attempt, pane, { selection, revealChildPath })
    if (!resolved) {
      return
    }
    recordResolvedListing(paneId)
    await runActiveItemsSortForPane(paneId)
    await panesStore().getState().finalizeListing(paneId)
  } catch (error) {
    if (isListingAttemptStale(attempt)) {
      return
    }
    log.error('reloadPane failed to load directory', { paneId, path: pane.path, error })
    panesStore().setState((state) => listingFailed(state, paneId, error))
  }
}

export async function navigatePane(paneId: PaneId, path: string, options: NavigateOptions = {}) {
  log.debug('navigatePane', { paneId, path, viaHistory: options.viaHistory ?? false })
  const currentTab = activeTab(paneId)
  const lockedElsewhere = currentTab.locked && !pathsMatch(currentTab.path, path)
  if (lockedElsewhere) {
    await panesStore().getState().openTabFromPath(paneId, path, { revealPath: options.revealPath })
    return
  }
  const viaHistory = options.viaHistory ?? false
  if (!viaHistory) {
    clearFilterTimer(paneId)
  }

  const leavingPath = panesStore().getState().panes[paneId].path
  parkRevealPath(paneId, revealTargetAfterNavigation(leavingPath, path, options.revealPath))
  releaseSizesOnLeave(paneId, path)

  panesStore().setState((state) => navigatedPanes(state, paneId, { path, viaHistory }))
  await panesStore().getState().reloadPane(paneId)
}

export async function goUp(paneId: PaneId) {
  const currentPath = panesStore().getState().panes[paneId].path
  const parentPath = getParentPath(currentPath)
  log.debug('goUp', { paneId, currentPath, parentPath })
  if (parentPath) {
    await panesStore().getState().navigatePane(paneId, parentPath)
  }
}

export async function goBack(paneId: PaneId) {
  const pane = panesStore().getState().panes[paneId]
  if (pane.historyIndex <= 0) {
    return
  }
  await stepThroughHistory(paneId, pane.historyIndex - 1, 'goBack')
}

export async function goForward(paneId: PaneId) {
  const pane = panesStore().getState().panes[paneId]
  const atHistoryEdge = pane.historyIndex < 0 || pane.historyIndex >= pane.history.length - 1
  if (atHistoryEdge) {
    return
  }
  await stepThroughHistory(paneId, pane.historyIndex + 1, 'goForward')
}

async function stepThroughHistory(paneId: PaneId, historyIndex: number, action: string) {
  const store = panesStore().getState()
  const path = store.panes[paneId].history[historyIndex]
  log.debug(action, { paneId, path })
  const tab = activeTab(paneId)
  const lockedElsewhere = tab.locked && !pathsMatch(tab.path, path)
  if (lockedElsewhere) {
    await store.openTabFromPath(paneId, path)
    return
  }
  panesStore().setState((state) => withHistoryIndex(state, paneId, historyIndex))
  await store.navigatePane(paneId, path, { viaHistory: true })
}

export async function finalizeListing(paneId: PaneId) {
  const pane = panesStore().getState().panes[paneId]
  if (isTrashPath(pane.path)) {
    return
  }

  try {
    await armTabWatch(paneId, pane)
    const folders = directoryPaths(panesStore().getState().panes[paneId].entries)
    await reportSizeSubscriptions(
      paneId,
      pane.path,
      folders.slice(0, AUTO_FOLDER_SIZE_MAX_DIRECTORIES),
    )
    const requested = await requestEagerDirectorySizes(folders)

    log.debug('finalizeListing done', {
      paneId,
      path: pane.path,
      entries: panesStore().getState().panes[paneId].entries.length,
      sizeRequests: requested.length,
    })

    await panesStore()
      .getState()
      .ensureTreeChildren(getParentPath(pane.path) ?? pane.path)
  } catch (error) {
    log.error('finalizeListing failed', { paneId, path: pane.path, error })
  }
}

export async function setSort(paneId: PaneId, sortKey: SortKey) {
  const pane = panesStore().getState().panes[paneId]
  if (!isListingSettled(pane)) {
    return
  }
  if (isTrashPath(pane.path)) {
    panesStore().setState((state) => trashSortedPanes(state, paneId, sortKey))
    return
  }

  panesStore().setState((state) => sortedPanes(state, paneId, sortKey))
  const nextPane = panesStore().getState().panes[paneId]
  useTabsStore.getState().patchActiveTab(paneId, {
    sortKey: nextPane.sortKey,
    sortDirection: nextPane.sortDirection,
  })
  schedulePersistSession(panesStore().getState().activePaneId)
  if (sortKey === 'items') {
    await runActiveItemsSortForPane(paneId)
  }
}

export function setFilterDraft(paneId: PaneId, value: string) {
  clearFilterTimer(paneId)
  panesStore().setState((state) => withFilterDraft(state, paneId, value))

  const nextTimer = window.setTimeout(() => {
    panesStore().setState((state) => withAppliedFilter(state, paneId, value))
    void panesStore().getState().reloadPane(paneId)
  }, FILTER_DELAY_MILLISECONDS)

  panesStore().setState((state) => withFilterTimer(state, paneId, nextTimer))
}

export function clearFilter(paneId: PaneId) {
  clearFilterTimer(paneId)
  panesStore().setState((state) => withClearedFilter(state, paneId))
  void panesStore().getState().reloadPane(paneId)
}

export function setFocusedEntry(paneId: PaneId, entryId: string | null) {
  panesStore().setState((state) => withFocusedEntry(state, paneId, entryId))
}

export function setScrollPosition(paneId: PaneId, path: string, scrollTop: number) {
  const previous = panesStore().getState().panes[paneId].scrollPositions[path]
  if (previous === scrollTop) {
    return
  }
  panesStore().setState((state) => withScrollPosition(state, paneId, { path, scrollTop }))
}

export async function setShowHiddenFiles(showHiddenFiles: boolean) {
  if (panesStore().getState().showHiddenFiles === showHiddenFiles) {
    return
  }
  panesStore().setState({ showHiddenFiles })
  await useConfigStore.getState().setShowHiddenFiles(showHiddenFiles)
  await Promise.all([
    panesStore().getState().reloadPane('left'),
    panesStore().getState().reloadPane('right'),
  ])
}

export function reset() {
  treeLoads.generation += 1
  treeLoads.flights.clear()
  clearRevealPaths()
  useTabsStore.getState().reset()
  void listingSessions.left.release()
  void listingSessions.right.release()
  panesStore().setState(defaultPanesData())
}
