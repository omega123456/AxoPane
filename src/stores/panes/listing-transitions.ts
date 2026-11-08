import type {
  DirectoryEntry,
  FolderSizeIndexStatus,
  SortDirection,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { pathsMatch } from '@/lib/path-compare'
import { getParentPath } from '@/stores/panes/pane-paths'
import { withResolvedIcons } from '@/stores/panes/icon-requests'
import { clearPassiveItemCountRequestsForPane } from '@/stores/panes/item-counts'
import {
  findMatchingEntryId,
  withPane,
  type PaneState,
  type PanesData,
  type PanesPatch,
  type TreeNodes,
} from '@/stores/panes/pane-state'

const FAILED_LISTING_MESSAGE = 'Failed to load directory'
const UNSEEDED_HISTORY_INDEX = -1

export type TabView = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
}

export type SelectionSnapshot = {
  focusedId: string | null
  selectedIds: string[]
}

export type ResolvedListing = {
  paneId: PaneId
  path: string
  sortedEntries: DirectoryEntry[]
  selection: SelectionSnapshot
}

export type ResolvedDirectoryListing = ResolvedListing & {
  requestId: number
  revealEntry: DirectoryEntry | undefined
  revealScrollRequestId: number
}

export type InitialPanes = {
  activePaneId: PaneId
  showHiddenFiles: boolean
  folderSizeIndexStatus: FolderSizeIndexStatus
  volumes: VolumeInfo[]
  leftTab: TabView
  rightTab: TabView
  treeRoots: string[]
  treeNodes: TreeNodes
}

export type ListingView = Pick<PaneState, 'path' | 'sortKey' | 'sortDirection' | 'filterApplied'>

export function listingViewOf(pane: PaneState): ListingView {
  return {
    path: pane.path,
    sortKey: pane.sortKey,
    sortDirection: pane.sortDirection,
    filterApplied: pane.filterApplied,
  }
}

export function matchesListingView(pane: PaneState, expected: ListingView) {
  return (
    pathsMatch(pane.path, expected.path) &&
    pane.sortKey === expected.sortKey &&
    pane.sortDirection === expected.sortDirection &&
    pane.filterApplied === expected.filterApplied
  )
}

export function revealTargetAfterNavigation(
  leavingPath: string,
  destinationPath: string,
  revealPath: string | undefined,
) {
  if (revealPath) {
    return revealPath
  }
  const leavingParentPath = getParentPath(leavingPath)
  const navigatingUp = leavingParentPath !== null && pathsMatch(leavingParentPath, destinationPath)
  return navigatingUp ? leavingPath : undefined
}

export function findRevealEntry(entries: DirectoryEntry[], revealPath: string | undefined) {
  if (!revealPath) {
    return undefined
  }
  return entries.find((entry) => pathsMatch(entry.path, revealPath))
}

export function tabViewOf(pane: PaneState): TabView {
  return {
    path: pane.path,
    sortKey: pane.sortKey,
    sortDirection: pane.sortDirection,
    filter: pane.filterApplied,
  }
}

export const DEFAULT_TAB_VIEW: Omit<TabView, 'path'> = {
  sortKey: 'name',
  sortDirection: 'asc',
  filter: '',
}

function initializedPane(pane: PaneState, tab: TabView, title: string): PaneState {
  return {
    ...pane,
    path: tab.path,
    sortKey: tab.sortKey,
    sortDirection: tab.sortDirection,
    filterDraft: tab.filter,
    filterApplied: tab.filter,
    title,
    history: [tab.path],
    historyIndex: 0,
  }
}

export function initializedPanes(state: PanesData, initial: InitialPanes): Partial<PanesData> {
  return {
    activePaneId: initial.activePaneId,
    showHiddenFiles: initial.showHiddenFiles,
    folderSizeIndexStatus: state.folderSizeIndexStatus ?? initial.folderSizeIndexStatus,
    volumes: initial.volumes,
    panes: {
      left: initializedPane(state.panes.left, initial.leftTab, 'Left pane'),
      right: initializedPane(state.panes.right, initial.rightTab, 'Right pane'),
    },
    treeRoots: initial.treeRoots,
    treeNodes: initial.treeNodes,
  }
}

export function listingStarted(state: PanesPatch, paneId: PaneId): PanesPatch {
  return withPane(state, paneId, {
    loading: true,
    error: null,
    listRequestId: 0,
    listingGeneration: state.panes[paneId].listingGeneration + 1,
    acceptedChunkIndexes: [],
    listingComplete: false,
  })
}

function seededHistory(previous: PaneState, path: string) {
  if (previous.historyIndex === UNSEEDED_HISTORY_INDEX) {
    return { history: [path], historyIndex: 0 }
  }
  return { history: previous.history, historyIndex: previous.historyIndex }
}

function restoredFocusId(previous: PaneState, listing: ResolvedListing) {
  return findMatchingEntryId(listing.sortedEntries, [
    listing.selection.focusedId,
    previous.focusedEntryId,
    ...listing.selection.selectedIds,
  ])
}

function resolvedPaneBase(
  state: Pick<PanesData, 'panes' | 'resolvedIconPaths'>,
  listing: ResolvedListing,
) {
  const previous = state.panes[listing.paneId]
  return {
    path: listing.path,
    entries: withResolvedIcons(listing.sortedEntries, state.resolvedIconPaths),
    loading: false,
    itemsSortStatus: 'idle' as const,
    ...seededHistory(previous, listing.path),
  }
}

export function trashListingResolved(
  state: Pick<PanesData, 'panes' | 'resolvedIconPaths'>,
  listing: ResolvedListing,
): PanesPatch {
  const previous = state.panes[listing.paneId]
  return withPane(state, listing.paneId, {
    ...resolvedPaneBase(state, listing),
    focusedEntryId: restoredFocusId(previous, listing) ?? listing.sortedEntries[0]?.id ?? null,
  })
}

export function directoryListingResolved(
  state: Pick<
    PanesData,
    'panes' | 'resolvedIconPaths' | 'passiveItemCountRequests' | 'revealScrollRequests'
  >,
  listing: ResolvedDirectoryListing,
): Partial<PanesData> {
  const { paneId, revealEntry } = listing
  const previous = state.panes[paneId]
  const focusedEntryId =
    revealEntry?.id ?? restoredFocusId(previous, listing) ?? listing.sortedEntries[0]?.id ?? null
  return {
    passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
      state.passiveItemCountRequests,
      paneId,
    ),
    revealScrollRequests: revealEntry
      ? { ...state.revealScrollRequests, [paneId]: listing.revealScrollRequestId }
      : state.revealScrollRequests,
    ...withPane(state, paneId, {
      ...resolvedPaneBase(state, listing),
      focusedEntryId,
      listRequestId: listing.requestId,
      listingComplete: true,
    }),
  }
}

export function listingFailed(state: PanesPatch, paneId: PaneId, error: unknown): PanesPatch {
  return withPane(state, paneId, {
    loading: false,
    itemsSortStatus: 'idle',
    error: error instanceof Error ? error.message : FAILED_LISTING_MESSAGE,
  })
}

function historyAfterNavigation(pane: PaneState, path: string) {
  const base = pane.historyIndex >= 0 ? pane.history.slice(0, pane.historyIndex + 1) : []
  const alreadyCurrent = base.at(-1) === path
  const history = alreadyCurrent ? base : [...base, path]
  return { history, historyIndex: history.length - 1 }
}

function withoutScrollPosition(scrollPositions: Record<string, number>, path: string) {
  return Object.fromEntries(
    Object.entries(scrollPositions).filter(([storedPath]) => storedPath !== path),
  )
}

export function navigatedPanes(
  state: Pick<PanesData, 'panes' | 'filterTimers'>,
  paneId: PaneId,
  path: string,
  viaHistory: boolean,
): Partial<PanesData> {
  if (viaHistory) {
    return withPane(state, paneId, { path })
  }

  const pane = state.panes[paneId]
  return {
    ...withPane(state, paneId, {
      path,
      filterDraft: '',
      filterApplied: '',
      typing: false,
      ...historyAfterNavigation(pane, path),
      scrollPositions: withoutScrollPosition(pane.scrollPositions, path),
    }),
    filterTimers: { ...state.filterTimers, [paneId]: undefined },
  }
}

export function withHistoryIndex(
  state: PanesPatch,
  paneId: PaneId,
  historyIndex: number,
): PanesPatch {
  return withPane(state, paneId, { historyIndex })
}

export function paneResetForTab(
  state: Pick<PanesData, 'panes' | 'passiveItemCountRequests'>,
  paneId: PaneId,
  tab: TabView,
): Pick<PanesData, 'panes' | 'passiveItemCountRequests'> {
  return {
    passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
      state.passiveItemCountRequests,
      paneId,
    ),
    ...withPane(state, paneId, {
      path: tab.path,
      sortKey: tab.sortKey,
      sortDirection: tab.sortDirection,
      filterDraft: tab.filter,
      filterApplied: tab.filter,
      typing: false,
      entries: [],
      itemsSortStatus: 'idle',
      focusedEntryId: null,
      scrollPositions: {},
      history: [],
      historyIndex: UNSEEDED_HISTORY_INDEX,
    }),
  }
}

export function focusRequested(
  state: Pick<PanesData, 'focusRequestId'>,
  paneId: PaneId,
): Pick<PanesData, 'activePaneId' | 'focusRequestId' | 'focusRequestPaneId'> {
  return {
    activePaneId: paneId,
    focusRequestId: state.focusRequestId + 1,
    focusRequestPaneId: paneId,
  }
}

export function withFilterDraft(
  state: PanesPatch,
  paneId: PaneId,
  filterDraft: string,
): PanesPatch {
  return withPane(state, paneId, { filterDraft, typing: true })
}

export function withAppliedFilter(
  state: PanesPatch,
  paneId: PaneId,
  filterApplied: string,
): PanesPatch {
  const pane = state.panes[paneId]
  return withPane(state, paneId, {
    filterApplied,
    typing: false,
    itemsSortStatus: pane.sortKey === 'items' ? 'counting' : pane.itemsSortStatus,
  })
}

export function withClearedFilter(
  state: Pick<PanesData, 'panes' | 'filterTimers'>,
  paneId: PaneId,
): Pick<PanesData, 'panes' | 'filterTimers'> {
  const applied = withAppliedFilter(state, paneId, '')
  return {
    filterTimers: { ...state.filterTimers, [paneId]: undefined },
    ...withPane(applied, paneId, { filterDraft: '' }),
  }
}

export function withFilterTimer(
  state: Pick<PanesData, 'filterTimers'>,
  paneId: PaneId,
  timer: number,
): Pick<PanesData, 'filterTimers'> {
  return { filterTimers: { ...state.filterTimers, [paneId]: timer } }
}

export function withFocusedEntry(
  state: PanesPatch,
  paneId: PaneId,
  focusedEntryId: string | null,
): PanesPatch {
  return withPane(state, paneId, { focusedEntryId })
}

export function withScrollPosition(
  state: PanesPatch,
  paneId: PaneId,
  path: string,
  scrollTop: number,
): PanesPatch {
  const scrollPositions = { ...state.panes[paneId].scrollPositions, [path]: scrollTop }
  return withPane(state, paneId, { scrollPositions })
}
