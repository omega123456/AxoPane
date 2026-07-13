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
  setTabWatch,
} from '@/lib/ipc/commands'
import { isTrashPath, trashEntryToDirectoryEntry } from '@/lib/trash'
import { SESSION_PAGE_SIZE } from '@/lib/types/ipc'
import type {
  DirectoryEntry,
  DirPatchEvent,
  EverythingStatus,
  IconStateEvent,
  ItemCountEvent,
  ItemCountRequestContext,
  SessionPatchEvent,
  SessionState,
  SessionViewParams,
  SizeStateEvent,
  SortDirection,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import { activeTab, fromSessionPane, toSessionPane, useTabsStore } from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
import { log } from '@/lib/app-log-commands'
import { pathKey, pathsMatch, samePathOrWindowsCaseFold } from '@/lib/path-compare'
import {
  findVolumeForPath,
  formatVolumeTreeName,
  isPathInsideVolume,
  sortVolumesForTree,
} from '@/lib/volumes'
import { useSelectionStore } from '@/stores/selection-store'
import { ListingSession } from '@/stores/panes/listing-session'
import { expandabilityOf, pruneTreeCache, type LazyTreeNode } from '@/stores/panes/tree-cache'
import {
  iconCacheNow,
  iconWeight,
  pruneIconCache,
  pruneSizeCache,
} from '@/stores/panes/cache-policy'

type PaneId = 'left' | 'right'
type SizeStateKind = SizeStateEvent['state']

// When `viaHistory` is set the navigation is a back/forward step and must not
// rewrite the pane's history stack (the index has already been moved).
type NavigateOptions = { viaHistory?: boolean }

export type TreeNodeState = Omit<LazyTreeNode, 'expandability' | 'lastAccess'> & {
  expandability?: LazyTreeNode['expandability']
  lastAccess?: number
}

type EntrySizeState = {
  state: SizeStateKind
  sizeBytes: number | null
  source: SizeStateEvent['source']
}

type PaneState = {
  id: PaneId
  title: string
  path: string
  entries: DirectoryEntry[]
  focusedEntryId: string | null
  sortKey: SortKey
  sortDirection: SortDirection
  filterDraft: string
  filterApplied: string
  typing: boolean
  loading: boolean
  itemsSortStatus: 'idle' | 'counting' | 'complete' | 'stale' | 'failed'
  // Monotonic frontend operation identity for explicit Items sorts. Context
  // alone is insufficient when the same pane context starts another sort.
  itemsSortOperationId: number
  error: string | null
  // Monotonic id of the in-flight streamed listing for this pane's active tab,
  // set from the `start_list_dir` head. Streamed `list_chunk` events are only
  // applied while their `requestId` matches, so chunks from a superseded
  // navigation are ignored.
  listRequestId: number
  // Local navigation generation invalidates the previously accepted request
  // before `start_list_dir` resolves. This closes the A -> B -> A tail race.
  listingGeneration: number
  // Stream chunks carry a protocol-level index, not inferred entry identity.
  acceptedChunkIndexes: number[]
  // A streamed listing is safe for backend Items sorting only after its final
  // chunk has been accepted. This also rejects duplicate/late tail chunks.
  listingComplete: boolean
  scrollPositions: Record<string, number>
  history: string[]
  historyIndex: number
}

type PendingSizeRequests = Record<string, true>
type PendingIconRequests = Record<string, true>
type PassiveItemCountRequests = Record<string, true>
// Paths that have received a `request_icons` response, keyed to the resolved
// data URL or `null` when the backend deliberately found no native icon. Most
// files never resolve to an icon (non-Windows, or a Windows extension outside
// the native-icon allowlist), so "no icon yet" cannot mean "still needs a
// request": without this permanent record the dedup below would re-request
// the same never-resolving files forever. Keeping the value also lets panes
// opened after the first response hydrate their rows from the cache.
type ResolvedIconPaths = Record<string, string | null>

type InitializePayload = {
  session: SessionState
  showHiddenFiles: boolean
  everythingStatus: EverythingStatus
  volumes: VolumeInfo[]
}

type PanesStore = {
  activePaneId: PaneId
  // Bumped whenever a pane should claim real DOM keyboard focus (not just
  // become the "active" pane in state), e.g. when the tree view opens a path
  // into a specific pane as a new tab.
  focusRequestId: number
  focusRequestPaneId: PaneId | null
  panes: Record<PaneId, PaneState>
  showHiddenFiles: boolean
  everythingStatus: EverythingStatus | null
  volumes: VolumeInfo[]
  sizeStates: Record<string, EntrySizeState>
  pendingSizeRequests: PendingSizeRequests
  pendingIconRequests: PendingIconRequests
  passiveItemCountRequests: PassiveItemCountRequests
  resolvedIconPaths: ResolvedIconPaths
  iconCacheTouched: Record<string, number>
  treeNodes: Record<string, TreeNodeState>
  treeRoots: string[]
  filterTimers: Partial<Record<PaneId, number>>
  // Bumped when a listing finished with a parent-reveal focus (navigating up
  // highlighted the folder we came from); `FilePane` scrolls the focused row
  // into view when its pane's counter changes.
  revealScrollRequests: Partial<Record<PaneId, number>>
  initialize: (payload: InitializePayload) => void
  setActivePane: (paneId: PaneId) => void
  reloadPane: (paneId: PaneId) => Promise<void>
  navigatePane: (paneId: PaneId, path: string, options?: NavigateOptions) => Promise<void>
  goUp: (paneId: PaneId) => Promise<void>
  goBack: (paneId: PaneId) => Promise<void>
  goForward: (paneId: PaneId) => Promise<void>
  openTabFromPath: (paneId: PaneId, path: string) => Promise<void>
  closeTab: (paneId: PaneId, tabId: string) => Promise<void>
  setTabLocked: (paneId: PaneId, tabId: string, locked: boolean) => void
  switchTab: (paneId: PaneId, tabId: string) => Promise<void>
  refreshEverything: (paneId: PaneId) => Promise<void>
  /** Runs the post-listing tail (arm fs-watch, eager folder sizes, tree children) once a listing is complete. */
  finalizeListing: (paneId: PaneId) => Promise<void>
  applyDirPatch: (event: DirPatchEvent) => void
  applySessionPatch: (event: SessionPatchEvent) => void
  setSort: (paneId: PaneId, sortKey: SortKey) => Promise<void>
  setFilterDraft: (paneId: PaneId, value: string) => void
  clearFilter: (paneId: PaneId) => void
  setFocusedEntry: (paneId: PaneId, entryId: string | null) => void
  setScrollPosition: (paneId: PaneId, path: string, scrollTop: number) => void
  /** Applies a batch of size-state events (coalesced by the frontend rAF batcher) in one `set`. */
  applySizeStates: (events: SizeStateEvent[]) => void
  requestVisibleItemCounts: (
    paneId: PaneId,
    visibleStartIndex: number,
    visibleEndIndex: number,
    viewportCount: number,
  ) => Promise<void>
  applyItemCountEvents: (events: ItemCountEvent[]) => void
  requestVisibleIcons: (paneId: PaneId, paths: string[]) => Promise<void>
  /**
   * Applies a batch of icon-state events (coalesced by the frontend rAF
   * batcher, or a backend batch, in one `set`). Returns the existing `panes`
   * reference unchanged when none of the buffered paths matched any entry in
   * either pane, so a no-op icon update never triggers a re-render (FR7).
   */
  applyIconStates: (events: IconStateEvent[]) => void
  setEverythingStatus: (status: EverythingStatus) => void
  setVolumes: (volumes: VolumeInfo[]) => void
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  requestManualSize: (paneId: PaneId, entryId: string) => Promise<void>
  calculateAllFolderSizes: (paneId: PaneId) => Promise<void>
  ensureTreeChildren: (path: string | null) => Promise<void>
  revealPath: (path: string | null) => Promise<void>
  toggleTreeNode: (path: string) => Promise<void>
  reset: () => void
}

const filterDelayMs = 180
const AUTO_VISIBLE_ITEM_COUNT_LIMIT = 200
// Each path has one owner while its direct children are loading. Callers share
// that promise, preventing double expands/reveals from issuing duplicate IPC.
const treeLoadFlights = new Map<string, Promise<void>>()
let treeLoadGeneration = 0

// When a navigation moves a pane to the direct parent of the folder it is
// leaving (Up/Backspace, the synthetic ".." row, or a history step that lands
// one level above), the leaving path is parked here so the parent's listing
// can select and reveal that folder once it resolves. Keyed per pane and
// consumed (cleared) by the next `reloadPane` for that pane, so a superseding
// navigation or an unrelated reload never applies a stale reveal.
const pendingParentRevealPaths: Partial<Record<PaneId, string>> = {}
let revealScrollRequestCounter = 0

// ---------------------------------------------------------------------------
// v2 directory-session integration (Phase 4 live-app wiring).
//
// One `ListingSession` per pane (matching Rust, which retires the prior
// session for a pane inside `begin_navigation` — see `ListingSession.begin`'s
// docstring) drives navigation instead of `start_list_dir`/`dir://list-chunk`.
// `panes-store.ts` still exposes `pane.entries: DirectoryEntry[]` as a fully
// resident flat array to the rest of the app (`FilePane`, `DetailsPanel`,
// `StatusBar`, selection, marquee, drag-and-drop, keyboard navigation) — none
// of that call surface needs to change, since `selection-store.ts` already
// tracks selection by entry id/path rather than array index.
//
// STOPGAP(phase-5): materializing the full listing eagerly (walking every v2
// page until `loadedRowCount === totalRows`) is the pragmatic bridge until
// Phase 5 teaches `FilePane`/marquee/drag-and-drop/keyboard navigation to
// read directly from a sparse `PaneEntryCollection` (see
// `ensureSessionFullyLoaded` below). This still avoids the old streamed-chunk
// event fan-out, and Rust's v2 session model already holds the fully sorted
// listing in memory server-side (paging only slices an already-materialized
// vector), so eagerly walking pages here does no more enumeration/sorting
// work than the old `start_list_dir` + chunk-stream path did — it just
// fetches it through the range API instead of the chunk-stream API.
// ---------------------------------------------------------------------------
// `ListingSession.onChange` fires synchronously from within `set()`-adjacent
// IPC resolution handlers (`adoptBeginResponse`, the range-request `.then`).
// Each pane's session is constructed once at module scope with a callback
// that just notifies whichever `ensureSessionFullyLoaded` walk is currently
// awaiting a change, rather than a store `set()` directly — the flat
// `pane.entries` materialization only ever happens explicitly, once, at the
// end of that walk (see `reloadPane`/`reviseSessionView` below), so a partial
// page arriving mid-walk never produces a half-updated `pane.entries`.
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

// Monotonic identity for each successful v2-backed listing, replacing the old
// `head.requestId` assigned by `start_list_dir`. Kept as a frontend-local
// counter (not read from the v2 response) purely to preserve the existing
// staleness-guard contract used by `finalizeListing`'s `set_tab_watch` seed
// reference and by `requestVisibleItemCounts`/`applyItemCountEvents`.
let listRequestIdCounter = 0

function currentViewParams(pane: PaneState, showHiddenFiles: boolean): SessionViewParams {
  return {
    sortKey: pane.sortKey,
    sortDirection: pane.sortDirection,
    filter: pane.filterApplied,
    showHidden: showHiddenFiles,
    includeItemCounts: false,
  }
}

/**
 * Waits for one `onChange` notification from `paneId`'s session (or resolves
 * immediately if `check()` already passes). Callers include terminal session
 * states in `check`, so failed range requests always wake the waiter.
 */
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

/**
 * STOPGAP(phase-5): walks every page of `session`'s active collection until
 * every row is loaded, pinning every page for the duration so the 12-page LRU
 * retention in `PaneEntryCollection` cannot evict an earlier page while a
 * later one is still being fetched (a real risk for directories over 6000
 * entries, since only the viewport +/- 1 page is pinned by default). Once
 * fully loaded, pins are reset back to just the viewport (page 0). Resolves
 * early if the session's baseline moves on (a newer navigation/view revision
 * superseded this walk) so a stale materialization request can never hang.
 * Phase 5's real sparse-rendering integration replaces this whole helper with
 * bounded, on-demand page loads driven by the visible viewport instead of
 * eagerly loading everything up front.
 */
async function ensureSessionFullyLoaded(paneId: PaneId, session: ListingSession): Promise<void> {
  const baselineAtStart = session.currentBaseline
  const { collection } = session
  if (collection.totalRows === 0 || !baselineAtStart) {
    return
  }

  const stillCurrent = () => session.currentBaseline === baselineAtStart
  const lastPageIndex = Math.floor((collection.totalRows - 1) / SESSION_PAGE_SIZE)
  const allPageIndexes = Array.from({ length: lastPageIndex + 1 }, (_, index) => index)
  collection.setPinnedPages(allPageIndexes)

  for (const pageIndex of allPageIndexes) {
    if (!stillCurrent()) {
      return
    }
    if (collection.isPageLoaded(pageIndex)) {
      continue
    }

    // Pages are requested and awaited one at a time on purpose:
    // `ensurePageLoaded` self-limits concurrent requests via
    // `MAX_IN_FLIGHT_REQUESTS`, so sequentially awaiting here still lets
    // multiple requests be in flight together while keeping this walk's own
    // bookkeeping (which page comes next) simple.
    session.ensurePageLoaded(pageIndex)
    await waitForSessionChange(
      paneId,
      () =>
        !stillCurrent() || collection.isPageLoaded(pageIndex) || session.currentStatus === 'error',
    )
    if (session.currentStatus === 'error') {
      throw new Error(session.currentError ?? 'Failed to load directory range')
    }
  }

  if (stillCurrent()) {
    session.pinViewportPages(0)
  }
}

function createPane(id: PaneId, title: string, path = '.'): PaneState {
  return {
    id,
    title,
    path,
    entries: [],
    focusedEntryId: null,
    sortKey: 'name',
    sortDirection: 'asc',
    filterDraft: '',
    filterApplied: '',
    typing: false,
    loading: false,
    itemsSortStatus: 'idle',
    itemsSortOperationId: 0,
    error: null,
    listRequestId: 0,
    listingGeneration: 0,
    acceptedChunkIndexes: [],
    listingComplete: true,
    scrollPositions: {},
    history: [],
    historyIndex: -1,
  }
}

function defaultState() {
  return {
    activePaneId: 'left' as PaneId,
    focusRequestId: 0,
    focusRequestPaneId: null as PaneId | null,
    panes: {
      left: createPane('left', 'Left pane'),
      right: createPane('right', 'Right pane'),
    },
    showHiddenFiles: false,
    everythingStatus: null,
    volumes: [] as VolumeInfo[],
    sizeStates: {} as Record<string, EntrySizeState>,
    pendingSizeRequests: {} as PendingSizeRequests,
    pendingIconRequests: {} as PendingIconRequests,
    passiveItemCountRequests: {} as PassiveItemCountRequests,
    resolvedIconPaths: {} as ResolvedIconPaths,
    iconCacheTouched: {} as Record<string, number>,
    treeNodes: {} as Record<string, TreeNodeState>,
    treeRoots: [] as string[],
    filterTimers: {} as Partial<Record<PaneId, number>>,
    revealScrollRequests: {} as Partial<Record<PaneId, number>>,
  }
}

function findMatchingEntryId(
  entries: DirectoryEntry[],
  candidates: Array<string | null | undefined>,
) {
  for (const candidate of candidates) {
    if (candidate && entries.some((entry) => entry.id === candidate)) {
      return candidate
    }
  }

  return null
}

export function getParentPath(path: string): string | null {
  if (!path) {
    return null
  }

  path = normalizeExtendedWindowsPath(path)

  // UNC share root: "\\server\share" is already the highest listable folder.
  if (/^\\\\[^\\/]+[\\/]?$/.test(path) || /^\\\\[^\\/]+[\\/][^\\/]+[\\/]?$/.test(path)) {
    return null
  }

  // Windows drive root: "C:\" or "C:" -> already at the top, cannot go up.
  if (/^[A-Za-z]:[\\/]?$/.test(path)) {
    return null
  }

  // POSIX root.
  if (path === '/') {
    return null
  }

  const stripped = path.replace(/[\\/]+$/, '')
  const lastSep = Math.max(stripped.lastIndexOf('\\'), stripped.lastIndexOf('/'))
  if (lastSep < 0) {
    return null
  }

  // Parent is a Windows drive root ("C:\Users" -> "C:\"): preserve the
  // trailing separator so the root remains a valid, listable path.
  const driveRoot = stripped.match(/^([A-Za-z]:)[\\/]/)
  if (driveRoot && lastSep === driveRoot[1].length) {
    return `${driveRoot[1]}\\`
  }

  // Parent is the POSIX root.
  if (lastSep === 0) {
    return '/'
  }

  return stripped.slice(0, lastSep)
}

/**
 * Last path segment, for naming a tree node created before it's ever been
 * seen as a listed child of its parent (e.g. expanded directly). Falls back
 * to the full (normalized) path only for roots with no separator at all.
 */
function basenameFromPath(path: string): string {
  const normalized = normalizeExtendedWindowsPath(path)
  const stripped = normalized.replace(/[\\/]+$/, '')
  const lastSep = Math.max(stripped.lastIndexOf('\\'), stripped.lastIndexOf('/'))
  if (lastSep < 0 || lastSep === stripped.length - 1) {
    return stripped || normalized
  }
  return stripped.slice(lastSep + 1)
}

function normalizeExtendedWindowsPath(path: string) {
  if (path.toLowerCase().startsWith('\\\\?\\unc\\')) {
    return `\\\\${path.slice(8)}`
  }

  if (/^\\\\\?\\[A-Za-z]:[\\/]/.test(path)) {
    return path.slice(4)
  }

  return path
}

function entrySortValue(
  entry: DirectoryEntry,
  sizeState: EntrySizeState | undefined,
  sortKey: SortKey,
) {
  if (sortKey === 'size') {
    return sizeState?.sizeBytes ?? entry.sizeBytes ?? -1
  }

  if (sortKey === 'items') {
    return entry.itemCount ?? -1
  }

  if (sortKey === 'type') {
    return entry.typeLabel.toLowerCase()
  }

  if (sortKey === 'modified') {
    return entry.modifiedAt ?? ''
  }

  if (sortKey === 'created') {
    return entry.createdAt ?? ''
  }

  return entry.name.toLowerCase()
}

function isKnownSize(entry: DirectoryEntry, sizeState: EntrySizeState | undefined) {
  return (sizeState?.sizeBytes ?? entry.sizeBytes) != null
}

function compareEntryNames(left: DirectoryEntry, right: DirectoryEntry) {
  return left.name.localeCompare(right.name, undefined, {
    numeric: true,
    sensitivity: 'base',
  })
}

function sortEntries(
  entries: DirectoryEntry[],
  sortKey: SortKey,
  sortDirection: SortDirection,
  sizeStates: Record<string, EntrySizeState>,
) {
  return [...entries].sort((left, right) =>
    compareEntries(left, right, sortKey, sortDirection, sizeStates),
  )
}

/**
 * The single ordering authority shared by the initial `sortEntries` sort and
 * the incremental patch inserts below. Folders always sort before files;
 * within a group, the active `sortKey`/`sortDirection` decides, with a
 * natural-name tiebreak. The size key keeps entries whose size is still
 * unknown after the ones with a known size (name-ordered among themselves) so
 * a folder whose size is still resolving never jumps around.
 */
function compareEntries(
  left: DirectoryEntry,
  right: DirectoryEntry,
  sortKey: SortKey,
  sortDirection: SortDirection,
  sizeStates: Record<string, EntrySizeState>,
): number {
  if (left.isDir !== right.isDir) {
    return left.isDir ? -1 : 1
  }

  const direction = sortDirection === 'asc' ? 1 : -1

  if (sortKey === 'size') {
    const leftHasKnownSize = isKnownSize(left, sizeStates[left.path])
    const rightHasKnownSize = isKnownSize(right, sizeStates[right.path])

    if (leftHasKnownSize !== rightHasKnownSize) {
      return leftHasKnownSize ? -1 : 1
    }

    if (!leftHasKnownSize && !rightHasKnownSize) {
      return compareEntryNames(left, right)
    }
  }

  const leftValue = entrySortValue(left, sizeStates[left.path], sortKey)
  const rightValue = entrySortValue(right, sizeStates[right.path], sortKey)

  if (leftValue < rightValue) {
    return -1 * direction
  }

  if (leftValue > rightValue) {
    return 1 * direction
  }

  return compareEntryNames(left, right) * direction
}

/**
 * Inserts `entry` into an already-sorted `entries` array at the position given
 * by [`compareEntries`], mutating the array in place. O(log n) comparisons +
 * one splice, so a watcher patch touching a handful of rows never triggers a
 * full re-sort of the whole listing.
 */
function insertSorted(
  entries: DirectoryEntry[],
  entry: DirectoryEntry,
  sortKey: SortKey,
  sortDirection: SortDirection,
  sizeStates: Record<string, EntrySizeState>,
) {
  let low = 0
  let high = entries.length
  while (low < high) {
    const mid = (low + high) >>> 1
    if (compareEntries(entries[mid], entry, sortKey, sortDirection, sizeStates) < 0) {
      low = mid + 1
    } else {
      high = mid
    }
  }
  entries.splice(low, 0, entry)
}

function directoryPaths(entries: DirectoryEntry[]): string[] {
  return entries.filter((entry) => entry.isDir).map((entry) => entry.path)
}

/**
 * Above this many folders in a single pane, eager (automatic) folder-size
 * calculation is suppressed for that pane and the manual "Calculate all
 * folder sizes" button is surfaced instead — computing hundreds of folder
 * sizes on every navigation is too expensive to run unprompted. The limit is
 * evaluated per pane, so one crowded pane never disables auto-sizing in the
 * other.
 */
export const AUTO_FOLDER_SIZE_MAX_DIRECTORIES = 500

// Diagnostic: cumulative count of size-state events applied, logged every
// `SIZE_EVENT_LOG_INTERVAL` so a flood of size updates (and whether it keeps
// arriving after navigation) is visible without one line per event.
let sizeEventsProcessed = 0
const SIZE_EVENT_LOG_INTERVAL = 2000

/**
 * Memoized `id -> entry` index for a pane's `entries` array, keyed by array
 * identity so repeated lookups against the same (immutable, Zustand-updated)
 * array reference reuse one `Map` instead of each caller re-scanning
 * `entries` with its own `.find()`. Building the index is still O(n), but it
 * happens at most once per distinct `entries` reference rather than once per
 * lookup — callers that need several ids from the same array (e.g.
 * `executeCommand` resolving the target, focused, and selected entries in
 * one dispatch) turn what was 2-3 separate O(n) scans into one O(n) build
 * plus O(1) lookups.
 */
const entryIndexByArray = new WeakMap<DirectoryEntry[], Map<string, DirectoryEntry>>()

export function indexEntriesById(entries: DirectoryEntry[]): Map<string, DirectoryEntry> {
  let index = entryIndexByArray.get(entries)
  if (!index) {
    index = new Map(entries.map((entry) => [entry.id, entry]))
    entryIndexByArray.set(entries, index)
  }
  return index
}

/** Number of folders in a pane, used to gate per-pane auto folder sizing. */
export function paneDirectoryCount(entries: DirectoryEntry[]): number {
  return entries.reduce((count, entry) => (entry.isDir ? count + 1 : count), 0)
}

/** True when a pane holds too many folders for eager folder sizing. */
export function autoFolderSizeDisabledForPane(entries: DirectoryEntry[]): boolean {
  return paneDirectoryCount(entries) > AUTO_FOLDER_SIZE_MAX_DIRECTORIES
}

function isTerminalSizeState(state: SizeStateKind) {
  return state === 'ready' || state === 'na' || state === 'error'
}

function shouldSkipSizeRequest(
  path: string,
  sizeStates: Record<string, EntrySizeState>,
  pendingSizeRequests: PendingSizeRequests,
) {
  if (pendingSizeRequests[path]) {
    return true
  }

  const sizeState = sizeStates[path]
  return (
    sizeState?.state === 'calculating' ||
    sizeState?.state === 'ready' ||
    sizeState?.state === 'na' ||
    sizeState?.state === 'error'
  )
}

function collectPendingSizeRequests(
  paths: string[],
  sizeStates: Record<string, EntrySizeState>,
  pendingSizeRequests: PendingSizeRequests,
) {
  const seen = new Set<string>()

  return paths.filter((path) => {
    if (seen.has(path)) {
      return false
    }
    seen.add(path)
    return !shouldSkipSizeRequest(path, sizeStates, pendingSizeRequests)
  })
}

function withPendingSizeRequests(
  pendingSizeRequests: PendingSizeRequests,
  paths: string[],
  value: boolean,
) {
  const next = { ...pendingSizeRequests }

  for (const path of paths) {
    if (value) {
      next[path] = true
    } else {
      delete next[path]
    }
  }

  return next
}

function collectPendingIconRequests(
  entries: DirectoryEntry[],
  paths: string[],
  pendingIconRequests: PendingIconRequests,
  resolvedIconPaths: ResolvedIconPaths,
) {
  const entryByPath = new Map(entries.map((entry) => [entry.path, entry]))
  const seen = new Set<string>()

  return paths.filter((path) => {
    if (seen.has(path)) {
      return false
    }
    seen.add(path)

    const entry = entryByPath.get(path)
    return (
      entry != null &&
      !entry.isDir &&
      entry.iconDataUrl == null &&
      !hasResolvedIconPath(resolvedIconPaths, path) &&
      !pendingIconRequests[path]
    )
  })
}

function hasResolvedIconPath(resolvedIconPaths: ResolvedIconPaths, path: string) {
  return Object.prototype.hasOwnProperty.call(resolvedIconPaths, path)
}

function itemCountContextKey(context: ItemCountRequestContext) {
  return `${context.paneId}::${context.tabId}::${context.requestId}`
}

function itemCountRequestKey(context: ItemCountRequestContext, path: string) {
  return `${itemCountContextKey(context)}::${pathKey(path)}`
}

function clearPassiveItemCountRequestsForPane(
  passiveItemCountRequests: PassiveItemCountRequests,
  paneId: PaneId,
) {
  const prefix = `${paneId}::`
  return Object.fromEntries(
    Object.entries(passiveItemCountRequests).filter(([key]) => !key.startsWith(prefix)),
  ) as PassiveItemCountRequests
}

function currentItemCountContext(paneId: PaneId, pane: PaneState): ItemCountRequestContext {
  return {
    paneId,
    tabId: activeTab(paneId).id,
    requestId: pane.listRequestId,
    path: pane.path,
  }
}

function itemCountContextMatches(
  paneId: PaneId,
  pane: PaneState,
  context: ItemCountRequestContext,
) {
  return (
    context.paneId === paneId &&
    context.tabId === activeTab(paneId).id &&
    context.requestId === pane.listRequestId &&
    pathsMatch(context.path, pane.path)
  )
}

function itemsSortContextMatches(
  paneId: PaneId,
  pane: PaneState,
  context: ItemCountRequestContext,
  expected?: { filterApplied?: string; sortDirection?: SortDirection },
) {
  return (
    itemCountContextMatches(paneId, pane, context) &&
    pane.sortKey === 'items' &&
    (expected?.filterApplied == null || pane.filterApplied === expected.filterApplied) &&
    (expected?.sortDirection == null || pane.sortDirection === expected.sortDirection)
  )
}

async function runActiveItemsSort(
  paneId: PaneId,
  context: ItemCountRequestContext,
  expected: {
    filterApplied: string
    sortDirection: SortDirection
  },
) {
  const state = usePanesStore.getState()
  const pane = state.panes[paneId]
  if (
    !itemsSortContextMatches(paneId, pane, context, expected) ||
    pane.loading ||
    !pane.listingComplete
  ) {
    return
  }

  let operationId = 0
  usePanesStore.setState((current) => {
    const currentPane = current.panes[paneId]
    if (!itemsSortContextMatches(paneId, currentPane, context, expected)) {
      return current
    }

    operationId = currentPane.itemsSortOperationId + 1
    return {
      panes: {
        ...current.panes,
        [paneId]: {
          ...currentPane,
          itemsSortStatus: 'counting',
          itemsSortOperationId: operationId,
          error: null,
        },
      },
    }
  })

  try {
    const response = await sortActiveItems({
      context,
      sortDirection: expected.sortDirection,
      filter: expected.filterApplied,
      showHidden: usePanesStore.getState().showHiddenFiles,
    })

    if (response.kind === 'superseded') {
      usePanesStore.setState((current) => {
        const currentPane = current.panes[paneId]
        if (
          !itemsSortContextMatches(paneId, currentPane, response.context, expected) ||
          currentPane.itemsSortOperationId !== operationId
        )
          return current
        return {
          panes: { ...current.panes, [paneId]: { ...currentPane, itemsSortStatus: 'stale' } },
        }
      })
      return
    }

    usePanesStore.setState((current) => {
      const currentPane = current.panes[paneId]
      if (
        !itemsSortContextMatches(paneId, currentPane, response.context, expected) ||
        currentPane.itemsSortOperationId !== operationId
      ) {
        return current
      }
      if (!samePathOrWindowsCaseFold(response.path, currentPane.path)) {
        return current
      }

      const nextEntries = withResolvedIcons(response.entries, current.resolvedIconPaths)
      const focusedStillPresent = nextEntries.some(
        (entry) => entry.id === currentPane.focusedEntryId,
      )

      return {
        panes: {
          ...current.panes,
          [paneId]: {
            ...currentPane,
            entries: nextEntries,
            focusedEntryId: focusedStillPresent
              ? currentPane.focusedEntryId
              : (nextEntries[0]?.id ?? null),
            itemsSortStatus: 'complete',
          },
        },
      }
    })
  } catch (error) {
    usePanesStore.setState((current) => {
      const currentPane = current.panes[paneId]
      if (
        !itemsSortContextMatches(paneId, currentPane, context, expected) ||
        currentPane.itemsSortOperationId !== operationId
      ) {
        return current
      }

      return {
        panes: {
          ...current.panes,
          [paneId]: {
            ...currentPane,
            itemsSortStatus: 'failed',
            error: error instanceof Error ? error.message : 'Failed to sort items',
          },
        },
      }
    })
  }
}

function withResolvedIcons(entries: DirectoryEntry[], resolvedIconPaths: ResolvedIconPaths) {
  let changed = false
  const nextEntries = entries.map((entry) =>
    hydrateEntryIcon(entry, resolvedIconPaths, () => {
      changed = true
    }),
  )

  return changed ? nextEntries : entries
}

/**
 * Applies any already-resolved native icon for a single entry from the session
 * icon cache. Used both by the bulk [`withResolvedIcons`] hydration and by the
 * incremental patch path, which only needs to hydrate the freshly-changed
 * entries rather than re-mapping the entire listing. `onChange` fires when a
 * new object had to be allocated.
 */
function hydrateEntryIcon(
  entry: DirectoryEntry,
  resolvedIconPaths: ResolvedIconPaths,
  onChange?: () => void,
): DirectoryEntry {
  if (entry.isDir || !hasResolvedIconPath(resolvedIconPaths, entry.path)) {
    return entry
  }

  const iconDataUrl = resolvedIconPaths[entry.path]
  if (entry.iconDataUrl === iconDataUrl) {
    return entry
  }

  onChange?.()
  return { ...entry, iconDataUrl }
}

function withPendingIconRequests(
  pendingIconRequests: PendingIconRequests,
  paths: string[],
  value: boolean,
) {
  const next = { ...pendingIconRequests }

  for (const path of paths) {
    if (value) {
      next[path] = true
    } else {
      delete next[path]
    }
  }

  return next
}

// Eager folder-size requests are only valid where the Everything index can
// answer them cheaply for the whole dataset (Windows + Everything available).
// On macOS and on Windows without Everything, sizes are strictly manual
// (Space / "Calculate size") — see plan FR7 / M4. Network paths are handled by
// the Rust size layer (always N/A); we never pre-gate on those here.
function eagerSizesEnabled(
  everythingStatus: EverythingStatus | null,
  autoFolderSize: boolean,
): boolean {
  return (everythingStatus?.isAvailable ?? false) && autoFolderSize
}

function paneIdFromTabId(tabId: string): PaneId {
  return tabId.startsWith('right') ? 'right' : 'left'
}

function buildRootTreeState(
  volumes: VolumeInfo[],
  currentNodes: Record<string, TreeNodeState> = {},
) {
  const sortedVolumes = sortVolumesForTree(volumes)
  const treeRoots = sortedVolumes.map((volume) => volume.mountRoot)
  const nextTreeNodes = Object.fromEntries(
    Object.entries(currentNodes).filter(([, node]) => {
      if (node.parentPath === null) {
        return false
      }

      return treeRoots.some((root) => isPathInsideVolume(node.path, root))
    }),
  ) as Record<string, TreeNodeState>

  for (const volume of sortedVolumes) {
    const existing = currentNodes[volume.mountRoot]
    nextTreeNodes[volume.mountRoot] = {
      id: volume.mountRoot,
      name: formatVolumeTreeName(volume),
      path: volume.mountRoot,
      parentPath: null,
      children: existing?.children ?? [],
      expanded: existing?.expanded ?? false,
      loaded: existing?.loaded ?? false,
      expandability: existing?.expandability ?? 'unknown',
      lastAccess: existing?.lastAccess ?? Date.now(),
    }
  }

  return { treeRoots, treeNodes: nextTreeNodes }
}

function treeRootForPath(treeRoots: string[], path: string) {
  return (
    treeRoots.find((root) => root === path) ??
    treeRoots.find((root) => pathsMatch(root, path)) ??
    null
  )
}

const sessionPersistDelayMs = 200
let sessionPersistTimer: number | undefined

function buildSessionState(activePaneId: PaneId): SessionState {
  return {
    activePane: activePaneId,
    leftPath: activeTab('left').path,
    rightPath: activeTab('right').path,
    left: toSessionPane('left'),
    right: toSessionPane('right'),
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
  }, sessionPersistDelayMs)
}

/**
 * Applies an incremental directory patch to a pane's listing without a full
 * reload or re-sort. Returns the next entry list or null when the patch does
 * not target the pane's current path.
 *
 * The existing listing is already sorted, so every path the patch touches
 * (removed or changed) is dropped in a single order-preserving pass, then each
 * surviving changed entry is binary-inserted back at its sorted position via
 * [`insertSorted`]. This avoids the O(n·log n) locale-aware re-sort of the
 * whole listing that a folder-size folder full of 20k entries would otherwise
 * pay on every ~150 ms watcher batch. Only the freshly-changed entries have
 * their cached native icon re-applied (untouched rows keep their existing
 * objects, and therefore their icons).
 */
function patchEntries(
  pane: PaneState,
  event: DirPatchEvent,
  sizeStates: Record<string, EntrySizeState>,
  showHiddenFiles: boolean,
  resolvedIconPaths: ResolvedIconPaths,
): DirectoryEntry[] | null {
  if (pane.path !== event.path) {
    return null
  }

  const affected = [...event.removed]
  for (const change of event.changed) {
    if (!affected.some((path) => pathsMatch(path, change.path))) {
      affected.push(change.path)
    }
  }

  const next =
    affected.length > 0
      ? pane.entries.filter((entry) => !affected.some((path) => pathsMatch(path, entry.path)))
      : [...pane.entries]

  // Reconcile the changed set by path (last write wins, matching the previous
  // map-based behaviour); a change whose entry is missing or no longer matches
  // the pane's filter/hidden rules stays dropped rather than re-inserted.
  const inserts = new Map<string, DirectoryEntry>()
  for (const change of event.changed) {
    const matchingKey = Array.from(inserts.keys()).find((path) => pathsMatch(path, change.path))
    if (matchingKey) inserts.delete(matchingKey)
    if (change.entry && entryMatchesPane(change.entry, pane, showHiddenFiles)) {
      inserts.set(change.entry.path, change.entry)
    }
  }

  const hydrated = Array.from(inserts.values(), (entry) =>
    hydrateEntryIcon(entry, resolvedIconPaths),
  )

  // Size ordering depends on folder sizes that resolve asynchronously and are
  // not continuously re-sorted into the listing, so the array can't be assumed
  // sorted against the current sizeStates — fall back to a full re-sort for
  // that one key. Active items sorting also waits for the backend-owned
  // count-then-sort pass, so watcher patches preserve the current local order
  // plus changed rows rather than finalizing a client-side items order.
  if (pane.sortKey === 'size') {
    return sortEntries([...next, ...hydrated], pane.sortKey, pane.sortDirection, sizeStates)
  }

  if (pane.sortKey === 'items') {
    return [...next, ...hydrated]
  }

  for (const entry of hydrated) {
    insertSorted(next, entry, pane.sortKey, pane.sortDirection, sizeStates)
  }

  return next
}

function entryMatchesPane(entry: DirectoryEntry, pane: PaneState, showHiddenFiles: boolean) {
  if (!showHiddenFiles && (entry.isHidden || entry.isSystem)) {
    return false
  }

  return (
    pane.filterApplied === '' || entry.name.toLowerCase().includes(pane.filterApplied.toLowerCase())
  )
}

function pathsEqual(left: string, right: string) {
  return pathsMatch(left, right)
}

function matchesListingContext(
  pane: PaneState,
  expected: {
    path: string
    sortKey: SortKey
    sortDirection: SortDirection
    filterApplied: string
  },
) {
  return (
    pathsMatch(pane.path, expected.path) &&
    pane.sortKey === expected.sortKey &&
    pane.sortDirection === expected.sortDirection &&
    pane.filterApplied === expected.filterApplied
  )
}

function treeNodeKeyForPath(treeNodes: Record<string, TreeNodeState>, path: string) {
  if (treeNodes[path]) {
    return path
  }

  return Object.keys(treeNodes).find((nodePath) => pathsEqual(nodePath, path)) ?? null
}

function childPathInList(children: string[], path: string) {
  return children.find((childPath) => pathsEqual(childPath, path)) ?? null
}

function collectTreeSubtreePaths(
  treeNodes: Record<string, TreeNodeState>,
  path: string,
  collected: Set<string>,
) {
  if (collected.has(path)) {
    return
  }

  collected.add(path)
  for (const childPath of treeNodes[path]?.children ?? []) {
    collectTreeSubtreePaths(treeNodes, childPath, collected)
  }
}

function compareTreeChildPaths(
  treeNodes: Record<string, TreeNodeState>,
  leftPath: string,
  rightPath: string,
) {
  const leftName = treeNodes[leftPath]?.name ?? basenameFromPath(leftPath)
  const rightName = treeNodes[rightPath]?.name ?? basenameFromPath(rightPath)
  return leftName.localeCompare(rightName, undefined, {
    numeric: true,
    sensitivity: 'base',
  })
}

function applyTreeDirPatch(state: PanesStore, event: DirPatchEvent) {
  const parentKey = treeNodeKeyForPath(state.treeNodes, event.path)
  if (!parentKey) {
    return null
  }

  const parent = state.treeNodes[parentKey]
  if (!parent.loaded) {
    return null
  }

  let nextTreeNodes = state.treeNodes
  let nextChildren = parent.children

  const mutableTreeNodes = () => {
    if (nextTreeNodes === state.treeNodes) {
      nextTreeNodes = { ...state.treeNodes }
    }
    return nextTreeNodes
  }

  const mutableChildren = () => {
    if (nextChildren === parent.children) {
      nextChildren = [...parent.children]
    }
    return nextChildren
  }

  const removeChild = (path: string, pruneTree = true) => {
    const childPath = childPathInList(nextChildren, path)
    if (childPath) {
      const children = mutableChildren()
      nextChildren = children.filter((candidate) => !pathsEqual(candidate, path))
    }

    if (!pruneTree) {
      return
    }

    const nodeKey = treeNodeKeyForPath(nextTreeNodes, path)
    if (!nodeKey) {
      return
    }

    const node = nextTreeNodes[nodeKey]
    if (!pathsEqual(node.parentPath ?? '', parent.path) && !childPath) {
      return
    }

    const doomed = new Set<string>()
    collectTreeSubtreePaths(nextTreeNodes, nodeKey, doomed)
    const treeNodes = mutableTreeNodes()
    for (const doomedPath of doomed) {
      delete treeNodes[doomedPath]
    }
  }

  for (const path of event.removed) {
    removeChild(path)
  }

  for (const change of event.changed) {
    const entry = change.entry
    const keepDirectory =
      entry?.isDir === true && (state.showHiddenFiles || (!entry.isHidden && !entry.isSystem))
    const existingKeyBeforeRemove = treeNodeKeyForPath(nextTreeNodes, change.path)
    const existingBeforeRemove =
      keepDirectory && existingKeyBeforeRemove ? nextTreeNodes[existingKeyBeforeRemove] : undefined
    removeChild(change.path, !keepDirectory)

    if (!keepDirectory) {
      continue
    }

    const existingKey = treeNodeKeyForPath(nextTreeNodes, entry.path)
    const existing = existingKey ? nextTreeNodes[existingKey] : undefined
    const treeNodes = mutableTreeNodes()
    if (existingKey && existingKey !== entry.path) {
      delete treeNodes[existingKey]
    }
    treeNodes[entry.path] = {
      id: entry.path,
      name: entry.name,
      path: entry.path,
      parentPath: parent.path,
      children: existingBeforeRemove?.children ?? existing?.children ?? [],
      expanded: existingBeforeRemove?.expanded ?? existing?.expanded ?? false,
      loaded: existingBeforeRemove?.loaded ?? existing?.loaded ?? false,
      expandability: existingBeforeRemove?.expandability ?? existing?.expandability ?? 'unknown',
      lastAccess: Date.now(),
    }

    if (!childPathInList(nextChildren, entry.path)) {
      mutableChildren().push(entry.path)
    }
  }

  if (nextChildren !== parent.children) {
    const treeNodes = mutableTreeNodes()
    const uniqueChildren = Array.from(new Set(nextChildren))
    treeNodes[parentKey] = {
      ...parent,
      children: uniqueChildren.sort((left, right) => compareTreeChildPaths(treeNodes, left, right)),
    }
  }

  return nextTreeNodes === state.treeNodes ? null : nextTreeNodes
}

export const usePanesStore = create<PanesStore>((set, get) => ({
  ...defaultState(),
  initialize: ({ session, showHiddenFiles, everythingStatus, volumes }) => {
    const tabsStore = useTabsStore.getState()
    const leftPane = session.left ?? {
      activeTabIndex: 0,
      tabs: [
        {
          path: session.leftPath || '.',
          sortKey: 'name',
          sortDirection: 'asc',
          filter: '',
          locked: false,
        },
      ],
    }
    const rightPane = session.right ?? {
      activeTabIndex: 0,
      tabs: [
        {
          path: session.rightPath || '.',
          sortKey: 'name',
          sortDirection: 'asc',
          filter: '',
          locked: false,
        },
      ],
    }
    tabsStore.hydrate('left', fromSessionPane('left', leftPane))
    tabsStore.hydrate('right', fromSessionPane('right', rightPane))

    const activeLeft = activeTab('left')
    const activeRight = activeTab('right')
    const { treeRoots, treeNodes } = buildRootTreeState(volumes)

    set((state) => ({
      ...state,
      activePaneId: session.activePane === 'right' ? 'right' : 'left',
      showHiddenFiles,
      everythingStatus,
      volumes,
      panes: {
        left: {
          ...state.panes.left,
          path: activeLeft.path,
          sortKey: activeLeft.sortKey,
          sortDirection: activeLeft.sortDirection,
          filterDraft: activeLeft.filter,
          filterApplied: activeLeft.filter,
          title: 'Left pane',
          history: [activeLeft.path],
          historyIndex: 0,
        },
        right: {
          ...state.panes.right,
          path: activeRight.path,
          sortKey: activeRight.sortKey,
          sortDirection: activeRight.sortDirection,
          filterDraft: activeRight.filter,
          filterApplied: activeRight.filter,
          title: 'Right pane',
          history: [activeRight.path],
          historyIndex: 0,
        },
      },
      treeRoots,
      treeNodes,
    }))
  },
  setActivePane: (paneId) => {
    set({ activePaneId: paneId })
    schedulePersistSession(paneId)
  },
  reloadPane: async (paneId) => {
    const pane = get().panes[paneId]
    // Consume the parked reveal target up front so it binds to exactly this
    // reload; a superseded/stale attempt drops it instead of leaking into a
    // later unrelated reload.
    const revealChildPath = pendingParentRevealPaths[paneId]
    delete pendingParentRevealPaths[paneId]
    const tabId = activeTab(paneId).id
    const requestContext = {
      path: pane.path,
      sortKey: pane.sortKey,
      sortDirection: pane.sortDirection,
      filterApplied: pane.filterApplied,
    }
    const selection = useSelectionStore.getState().selections[paneId]
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          loading: true,
          error: null,
          // A previous same-path request must be invalid before this async IPC
          // call begins; its late chunks are never eligible for this attempt.
          listRequestId: 0,
          listingGeneration: state.panes[paneId].listingGeneration + 1,
          acceptedChunkIndexes: [],
          listingComplete: false,
        },
      },
    }))
    const listingGeneration = get().panes[paneId].listingGeneration

    try {
      // The trash browser is a virtual location backed by list_trash, not a
      // real directory: no fs watcher, tree-children lookup, or size requests
      // apply to it.
      if (isTrashPath(pane.path)) {
        const response = await listTrash()
        const sortedEntries = sortEntries(
          response.entries.map(trashEntryToDirectoryEntry),
          pane.sortKey,
          pane.sortDirection,
          get().sizeStates,
        )

        set((state) => {
          const previous = state.panes[paneId]
          const entries = withResolvedIcons(sortedEntries, state.resolvedIconPaths)
          const restoredFocusId = findMatchingEntryId(sortedEntries, [
            selection.focusedId,
            previous.focusedEntryId,
            ...selection.selectedIds,
          ])
          const seeded =
            previous.historyIndex === -1
              ? { history: [pane.path], historyIndex: 0 }
              : { history: previous.history, historyIndex: previous.historyIndex }

          return {
            panes: {
              ...state.panes,
              [paneId]: {
                ...previous,
                path: pane.path,
                entries,
                focusedEntryId: restoredFocusId ?? sortedEntries[0]?.id ?? null,
                loading: false,
                itemsSortStatus: 'idle',
                ...seeded,
              },
            },
          }
        })

        const nextTrashPane = get().panes[paneId]
        useTabsStore.getState().patchActiveTab(paneId, {
          path: nextTrashPane.path,
          sortKey: nextTrashPane.sortKey,
          sortDirection: nextTrashPane.sortDirection,
          filter: nextTrashPane.filterApplied,
        })
        schedulePersistSession(get().activePaneId)
        return
      }

      // v2 seekable directory session: Rust enumerates + sorts the whole
      // listing once on `begin_directory_session` and holds it server-side;
      // the frontend walks it page-by-page via `getDirectorySessionRange`
      // (STOPGAP(phase-5): eagerly, via `ensureSessionFullyLoaded`, until
      // `pane.entries` is taught to read a sparse `PaneEntryCollection`
      // directly) instead of the old `start_list_dir` + `dir://list-chunk`
      // stream. A path change (or the pane/tab's very first listing) begins a
      // fresh session; an unchanged path with a different sort/filter/hidden
      // view revises the already-active session instead, per the plan's
      // "path changes create a new session; sort/filter changes revise the
      // current session's view".
      const session = listingSessions[paneId]
      const view = currentViewParams(pane, get().showHiddenFiles)
      const isSamePathRevision =
        session.currentBaseline != null &&
        session.currentPath != null &&
        pathsMatch(session.currentPath, pane.path) &&
        activeTab(paneId).id === tabId
      if (isSamePathRevision) {
        await session.reviseView(view)
      } else {
        await session.begin(tabId, pane.path, view)
      }

      const currentTabId = activeTab(paneId).id
      const currentPane = get().panes[paneId]
      if (
        currentTabId !== tabId ||
        currentPane.listingGeneration !== listingGeneration ||
        !matchesListingContext(currentPane, requestContext)
      ) {
        return
      }

      if (session.currentStatus === 'error') {
        throw new Error(session.currentError ?? 'Failed to load directory')
      }

      await ensureSessionFullyLoaded(paneId, session)

      // Re-check staleness after the (possibly multi-page) materialization
      // walk: a newer navigation/sort/filter may have superseded this one
      // while pages were still loading.
      if (
        activeTab(paneId).id !== tabId ||
        get().panes[paneId].listingGeneration !== listingGeneration ||
        !matchesListingContext(get().panes[paneId], requestContext) ||
        session.currentBaseline == null
      ) {
        return
      }

      const resolvedPath = session.currentPath ?? pane.path
      // STOPGAP(phase-5): Rust is the canonical sort authority for every key
      // the v2 `SessionViewParams` contract knows about, but folder sizes
      // resolve asynchronously after the listing (and are not a
      // `SessionViewParams` field), so the size key still needs the same
      // client-side re-sort of the resident rows the v1 path used, applied
      // here to the fully-materialized `loadedEntries` instead of a streamed
      // chunk array. Items sort is unaffected — it already runs as a
      // separate backend count-then-sort pass via `runActiveItemsSort` below,
      // which replaces `pane.entries` wholesale once ready.
      const loadedEntries = session.loadedEntries()
      const sortedEntries =
        pane.sortKey === 'size'
          ? sortEntries(loadedEntries, pane.sortKey, pane.sortDirection, get().sizeStates)
          : loadedEntries

      // A monotonic identity for this successful listing, matching the old
      // `head.requestId` semantics: `finalizeListing`/`requestVisibleItemCounts`
      // compare it to reject stale item-count/watch-seed responses.
      listRequestIdCounter += 1
      const requestId = listRequestIdCounter

      // Navigating up to this folder's listing highlights the folder we came
      // from: it wins over the generic focus restore below, and also becomes
      // the selection so the row is visibly highlighted, not just focused.
      const revealEntry = revealChildPath
        ? sortedEntries.find((entry) => entry.isDir && pathsMatch(entry.path, revealChildPath))
        : undefined
      if (revealEntry) {
        revealScrollRequestCounter += 1
      }
      const revealScrollRequestId = revealScrollRequestCounter

      set((state) => {
        const previous = state.panes[paneId]
        const entries = withResolvedIcons(sortedEntries, state.resolvedIconPaths)
        const restoredFocusId = findMatchingEntryId(sortedEntries, [
          selection.focusedId,
          previous.focusedEntryId,
          ...selection.selectedIds,
        ])
        // Seed the back/forward stack the first time a pane resolves a real path
        // (initial load, or after a tab switch reset its history to empty).
        const seeded =
          previous.historyIndex === -1
            ? { history: [resolvedPath], historyIndex: 0 }
            : { history: previous.history, historyIndex: previous.historyIndex }

        return {
          passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
            state.passiveItemCountRequests,
            paneId,
          ),
          revealScrollRequests: revealEntry
            ? { ...state.revealScrollRequests, [paneId]: revealScrollRequestId }
            : state.revealScrollRequests,
          panes: {
            ...state.panes,
            [paneId]: {
              ...previous,
              path: resolvedPath,
              entries,
              focusedEntryId: revealEntry?.id ?? restoredFocusId ?? sortedEntries[0]?.id ?? null,
              loading: false,
              itemsSortStatus: 'idle',
              listRequestId: requestId,
              listingComplete: true,
              ...seeded,
            },
          },
        }
      })

      if (revealEntry) {
        useSelectionStore
          .getState()
          .setSelection(paneId, [revealEntry.id], revealEntry.id, revealEntry.id)
      }

      const nextPane = get().panes[paneId]
      useTabsStore.getState().patchActiveTab(paneId, {
        path: nextPane.path,
        sortKey: nextPane.sortKey,
        sortDirection: nextPane.sortDirection,
        filter: nextPane.filterApplied,
      })
      schedulePersistSession(get().activePaneId)

      await runActiveItemsSort(paneId, currentItemCountContext(paneId, nextPane), {
        filterApplied: nextPane.filterApplied,
        sortDirection: nextPane.sortDirection,
      })
      await get().finalizeListing(paneId)
    } catch (error) {
      const currentTabId = activeTab(paneId).id
      const currentPane = get().panes[paneId]
      if (
        currentTabId !== tabId ||
        currentPane.listingGeneration !== listingGeneration ||
        !matchesListingContext(currentPane, requestContext)
      ) {
        return
      }

      log.error('reloadPane failed to load directory', {
        paneId,
        path: requestContext.path,
        error,
      })
      set((state) => ({
        panes: {
          ...state.panes,
          [paneId]: {
            ...state.panes[paneId],
            loading: false,
            itemsSortStatus: 'idle',
            error: error instanceof Error ? error.message : 'Failed to load directory',
          },
        },
      }))
    }
  },
  navigatePane: async (paneId, path, options = {}) => {
    log.debug('navigatePane', { paneId, path, viaHistory: options.viaHistory ?? false })
    const currentTab = activeTab(paneId)
    if (currentTab.locked && !pathsMatch(currentTab.path, path)) {
      await get().openTabFromPath(paneId, path)
      return
    }
    const timer = get().filterTimers[paneId]
    if (timer && !options.viaHistory) {
      window.clearTimeout(timer)
    }

    // Abandon in-flight folder-size jobs for the folder we're leaving so they
    // stop churning once the pane moves on. Jobs for folders still shown by the
    // other pane (same directory open in both) are left untouched.
    const leaving = get().panes[paneId]

    // Navigating up one level should land with the folder we came from
    // selected and focused, matching Explorer/xplorer2. Park the leaving path
    // for `reloadPane` to resolve against the parent's fresh listing.
    const leavingParentPath = getParentPath(leaving.path)
    if (leavingParentPath && pathsMatch(leavingParentPath, path)) {
      pendingParentRevealPaths[paneId] = leaving.path
    } else {
      delete pendingParentRevealPaths[paneId]
    }

    const otherPaneId: PaneId = paneId === 'left' ? 'right' : 'left'
    const otherDirPaths = new Set(directoryPaths(get().panes[otherPaneId].entries))
    const leavingDirPaths = directoryPaths(leaving.entries)
    const staleSizePaths = leavingDirPaths.filter(
      (candidate) =>
        !otherDirPaths.has(candidate) &&
        (get().pendingSizeRequests[candidate] ||
          get().sizeStates[candidate]?.state === 'calculating'),
    )
    log.debug('size: navigate cancel evaluation', {
      paneId,
      from: leaving.path,
      to: path,
      leavingDirs: leavingDirPaths.length,
      stale: staleSizePaths.length,
      sampleStale: staleSizePaths.slice(0, 3),
    })
    if (staleSizePaths.length > 0) {
      set((state) => {
        const pendingSizeRequests = { ...state.pendingSizeRequests }
        const sizeStates = { ...state.sizeStates }
        for (const stalePath of staleSizePaths) {
          delete pendingSizeRequests[stalePath]
          // Drop the transient "calculating" marker so revisiting the folder
          // re-requests the size instead of assuming it is still resolving.
          if (sizeStates[stalePath]?.state === 'calculating') {
            delete sizeStates[stalePath]
          }
        }
        return { pendingSizeRequests, sizeStates }
      })
      const cancelStartedAt = performance.now()
      void cancelSizes(staleSizePaths)
        .then((response) => {
          log.debug('size: navigate cancel result', {
            paneId,
            requested: staleSizePaths.length,
            cancelled: response.cancelled,
            ms: Math.round(performance.now() - cancelStartedAt),
          })
        })
        .catch((error) => {
          log.error('navigatePane failed to cancel folder sizes', { paneId, error })
        })
    }

    set((state) => {
      const pane = state.panes[paneId]
      let { history, historyIndex } = pane

      // A fresh navigation truncates any forward history and appends the target,
      // skipping a no-op push when we are already sitting on that path. A
      // back/forward step (viaHistory) leaves the stack untouched.
      if (!options.viaHistory) {
        const base = historyIndex >= 0 ? history.slice(0, historyIndex + 1) : []
        history = base[base.length - 1] === path ? base : [...base, path]
        historyIndex = history.length - 1
      }

      return {
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            path,
            filterDraft: options.viaHistory ? pane.filterDraft : '',
            filterApplied: options.viaHistory ? pane.filterApplied : '',
            typing: options.viaHistory ? pane.typing : false,
            history,
            historyIndex,
            scrollPositions: options.viaHistory
              ? pane.scrollPositions
              : Object.fromEntries(
                  Object.entries(pane.scrollPositions).filter(
                    ([storedPath]) => storedPath !== path,
                  ),
                ),
          },
        },
        filterTimers: options.viaHistory
          ? state.filterTimers
          : { ...state.filterTimers, [paneId]: undefined },
      }
    })
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

    const nextIndex = pane.historyIndex - 1
    const path = pane.history[nextIndex]
    log.debug('goBack', { paneId, path })
    if (activeTab(paneId).locked && !pathsMatch(activeTab(paneId).path, path)) {
      await get().openTabFromPath(paneId, path)
      return
    }
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: { ...state.panes[paneId], historyIndex: nextIndex },
      },
    }))
    await get().navigatePane(paneId, path, { viaHistory: true })
  },
  goForward: async (paneId) => {
    const pane = get().panes[paneId]
    if (pane.historyIndex < 0 || pane.historyIndex >= pane.history.length - 1) {
      return
    }

    const nextIndex = pane.historyIndex + 1
    const path = pane.history[nextIndex]
    log.debug('goForward', { paneId, path })
    if (activeTab(paneId).locked && !pathsMatch(activeTab(paneId).path, path)) {
      await get().openTabFromPath(paneId, path)
      return
    }
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: { ...state.panes[paneId], historyIndex: nextIndex },
      },
    }))
    await get().navigatePane(paneId, path, { viaHistory: true })
  },
  openTabFromPath: async (paneId, path) => {
    log.debug('openTabFromPath', { paneId, path })
    useTabsStore.getState().addTab(paneId, {
      path,
      sortKey: 'name',
      sortDirection: 'asc',
      filter: '',
      locked: false,
    })

    set((state) => ({
      activePaneId: paneId,
      focusRequestId: state.focusRequestId + 1,
      focusRequestPaneId: paneId,
      passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
        state.passiveItemCountRequests,
        paneId,
      ),
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          path,
          sortKey: 'name',
          sortDirection: 'asc',
          filterDraft: '',
          filterApplied: '',
          typing: false,
          entries: [],
          itemsSortStatus: 'idle',
          focusedEntryId: null,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

    // Best-effort: release the previously active tab's v2 session before the
    // new tab's `begin_directory_session` replaces it.
    void listingSessions[paneId].release()
    await get().reloadPane(paneId)
  },
  closeTab: async (paneId, tabId) => {
    const before = useTabsStore.getState().panes[paneId]
    const closingTab = before.tabs.find((tab) => tab.id === tabId)
    if (before.tabs.length <= 1 || !closingTab || closingTab.locked) {
      return
    }

    const wasActive = before.tabs[before.activeTabIndex]?.id === tabId
    useTabsStore.getState().closeTab(paneId, tabId)
    schedulePersistSession(get().activePaneId)

    if (!wasActive) {
      return
    }

    const next = activeTab(paneId)
    set((state) => ({
      passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
        state.passiveItemCountRequests,
        paneId,
      ),
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          path: next.path,
          sortKey: next.sortKey,
          sortDirection: next.sortDirection,
          filterDraft: next.filter,
          filterApplied: next.filter,
          typing: false,
          entries: [],
          itemsSortStatus: 'idle',
          focusedEntryId: null,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

    // Best-effort: release the closed tab's v2 session so Rust can drop it
    // promptly rather than waiting for the pane's next `begin` to replace it.
    void listingSessions[paneId].release()
    await get().reloadPane(paneId)
  },
  setTabLocked: (paneId, tabId, locked) => {
    useTabsStore.getState().setTabLocked(paneId, tabId, locked)
    schedulePersistSession(get().activePaneId)
  },
  switchTab: async (paneId, tabId) => {
    const before = useTabsStore.getState().panes[paneId]
    if (before.tabs[before.activeTabIndex]?.id === tabId) {
      set({ activePaneId: paneId })
      return
    }

    useTabsStore.getState().setActiveTab(paneId, tabId)
    schedulePersistSession(paneId)

    const next = activeTab(paneId)
    set((state) => ({
      activePaneId: paneId,
      passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
        state.passiveItemCountRequests,
        paneId,
      ),
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          path: next.path,
          sortKey: next.sortKey,
          sortDirection: next.sortDirection,
          filterDraft: next.filter,
          filterApplied: next.filter,
          typing: false,
          entries: [],
          itemsSortStatus: 'idle',
          focusedEntryId: null,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

    // Best-effort: release the previous tab's v2 session before switching so
    // its stale rows can never bleed into the newly active tab's collection.
    void listingSessions[paneId].release()
    // `reloadPane` performs the single directory enumeration for the newly
    // active tab and re-arms its watcher (seeded from that same listing), so
    // no separate recheck/patch step is needed here.
    await get().reloadPane(paneId)
  },
  refreshEverything: async (paneId) => {
    // Ctrl+R: drop cached size states for the active path then reload, which
    // re-requests folder sizes from scratch.
    const pane = get().panes[paneId]
    set((state) => {
      const nextSizeStates = { ...state.sizeStates }
      const pendingSizeRequests = { ...state.pendingSizeRequests }
      for (const entry of pane.entries) {
        delete nextSizeStates[entry.path]
        delete pendingSizeRequests[entry.path]
      }
      return { sizeStates: nextSizeStates, pendingSizeRequests }
    })

    // `revise_directory_session_view` re-derives sort/filter from Rust's
    // already-cached enumeration snapshot without touching the filesystem
    // again — wrong for an explicit "refresh from disk" action. Releasing the
    // session first forces `reloadPane`'s same-path check to fall through to
    // `begin_directory_session`, which re-enumerates.
    await listingSessions[paneId].release()
    await get().reloadPane(paneId)
  },
  finalizeListing: async (paneId) => {
    const pane = get().panes[paneId]
    // Trash is a virtual location with no fs watcher / tree / size requests.
    if (isTrashPath(pane.path)) {
      return
    }

    const requestDirectorySizes = async (paths: string[]) => {
      if (!eagerSizesEnabled(get().everythingStatus, useConfigStore.getState().autoFolderSize)) {
        return []
      }

      // Suppress eager sizing when the folder holds more directories than a
      // single pane can auto-calculate; the manual "Calculate all" button takes
      // over for that pane instead.
      if (paths.length > AUTO_FOLDER_SIZE_MAX_DIRECTORIES) {
        return []
      }

      const pending = collectPendingSizeRequests(paths, get().sizeStates, get().pendingSizeRequests)
      if (pending.length === 0) {
        return []
      }

      set((state) => ({
        pendingSizeRequests: withPendingSizeRequests(state.pendingSizeRequests, pending, true),
      }))

      try {
        await requestFolderSizes({ paths: pending })
        return pending
      } catch (error) {
        set((state) => ({
          pendingSizeRequests: withPendingSizeRequests(state.pendingSizeRequests, pending, false),
        }))
        throw error
      }
    }

    try {
      // Seed the fs-watch baseline on the Rust side (entries omitted): Rust
      // re-enumerates via `snapshot_for_target` on its worker thread instead of
      // us re-serializing the entire listing back across the IPC boundary.
      await setTabWatch(
        {
          tabId: activeTab(paneId).id,
          path: pane.path,
          sortKey: pane.sortKey,
          sortDirection: pane.sortDirection,
          filter: pane.filterApplied,
          showHidden: get().showHiddenFiles,
          includeItemCounts: false,
        },
        undefined,
        {
          tabId: activeTab(paneId).id,
          requestId: pane.listRequestId,
          path: pane.path,
        },
      )

      const requested = await requestDirectorySizes(directoryPaths(get().panes[paneId].entries))

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
  // STOPGAP(phase-5): `dir://patch` watcher events are applied by splicing
  // the affected rows directly into `pane.entries` in place (see
  // `patchEntries` below), not by tearing down and re-running
  // `beginDirectorySession`/`ListingSession.begin` for the pane on every
  // watch tick — avoiding exactly the full-session-refetch-on-every-mutation
  // regression the stopgap policy calls out. The v2 `ListingSession`'s
  // `PaneEntryCollection` is intentionally left un-patched here (it has no
  // incremental single-row patch API yet — only a full-page `installPage`
  // replace); it is fully re-materialized from a fresh
  // `begin_directory_session`/`reviseDirectorySessionView` call the next time
  // `reloadPane` runs for this pane (navigation, sort/filter change, or
  // explicit refresh), so it can only ever be transiently behind
  // `pane.entries` between a watch patch and the pane's next reload. Phase 5
  // should add a real incremental patch method to `PaneEntryCollection` and
  // apply patches to both here in the same pass.
  applyDirPatch: (event) =>
    set((state) => {
      const paneId = paneIdFromTabId(event.tabId)
      const pane = state.panes[paneId]

      // Pause reflow while the user is actively typing a filter; the next
      // reload after the filter settles will reconcile the listing.
      if (pane.typing) {
        log.debug('applyDirPatch skipped (typing)', { paneId, path: event.path })
        return state
      }

      // `patchEntries` already hydrates the cached native icon for the changed
      // entries it inserts, so no second full-listing `withResolvedIcons` pass
      // is needed here.
      if (event.tabId !== activeTab(paneId).id) {
        log.debug('applyDirPatch skipped (inactive tab)', { paneId, tabId: event.tabId })
        return state
      }

      // Items sorting is backend-owned count-then-sort. Preserve the accepted
      // rows verbatim until that replacement arrives; locally removing or
      // inserting patch rows would show an untrustworthy transient order.
      if (pane.sortKey === 'items') {
        if (!pathsMatch(pane.path, event.path)) {
          log.debug('applyDirPatch skipped (path mismatch)', {
            paneId,
            panePath: pane.path,
            eventPath: event.path,
          })
          return state
        }
        const treeNodes = applyTreeDirPatch(state, event)
        queueMicrotask(() => {
          const currentPane = usePanesStore.getState().panes[paneId]
          void runActiveItemsSort(paneId, currentItemCountContext(paneId, currentPane), {
            filterApplied: currentPane.filterApplied,
            sortDirection: currentPane.sortDirection,
          })
        })
        return {
          panes: { ...state.panes, [paneId]: { ...pane, itemsSortStatus: 'counting' } },
          ...(treeNodes ? { treeNodes } : {}),
        }
      }

      const entries = patchEntries(
        pane,
        event,
        state.sizeStates,
        state.showHiddenFiles,
        state.resolvedIconPaths,
      )
      if (!entries) {
        log.debug('applyDirPatch skipped (path mismatch)', {
          paneId,
          panePath: pane.path,
          eventPath: event.path,
        })
        return state
      }

      log.debug('applyDirPatch applied', {
        paneId,
        path: event.path,
        entries: entries.length,
        changed: event.changed.length,
        removed: event.removed.length,
      })

      const focusedStillPresent = entries.some((entry) => entry.id === pane.focusedEntryId)
      const treeNodes = applyTreeDirPatch(state, event)

      return {
        passiveItemCountRequests: (() => {
          const invalidatedPaths = new Set(
            event.changed
              .filter((change) => change.entry?.isDir && change.entry.itemCount == null)
              .map((change) =>
                itemCountRequestKey(currentItemCountContext(paneId, pane), change.path),
              ),
          )
          if (invalidatedPaths.size === 0) {
            return state.passiveItemCountRequests
          }
          return Object.fromEntries(
            Object.entries(state.passiveItemCountRequests).filter(
              ([key]) => !invalidatedPaths.has(key),
            ),
          ) as PassiveItemCountRequests
        })(),
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            entries,
            itemsSortStatus: pane.itemsSortStatus,
            focusedEntryId: focusedStillPresent ? pane.focusedEntryId : (entries[0]?.id ?? null),
          },
        },
        ...(treeNodes ? { treeNodes } : {}),
      }
    }),
  applySessionPatch: (event) => {
    const paneId = event.paneId === 'left' || event.paneId === 'right' ? event.paneId : null
    if (!paneId || activeTab(paneId).id !== event.tabId) {
      return
    }
    const session = listingSessions[paneId]
    const pane = get().panes[paneId]
    if (!pathsMatch(pane.path, event.path) || !session.applyPatch(event)) {
      return
    }

    // Apply the renderer-facing rows and the lazy tree only after the same
    // baseline gate above has accepted the session patch.  This prevents one
    // stale event from updating either half independently.
    if (event.mode === 'replaceView') {
      void get().reloadPane(paneId)
      return
    }
    const changed =
      event.mode === 'metadataOnly'
        ? event.updates.map((update) => ({ path: update.path, entry: update.entry }))
        : event.deltas
            .filter(
              (delta): delta is Extract<typeof delta, { entry: DirectoryEntry }> =>
                'entry' in delta,
            )
            .map((delta) => ({ path: delta.entry.path, entry: delta.entry }))
    const removed =
      event.mode === 'delta'
        ? event.deltas.filter((delta) => delta.kind === 'removed').map((delta) => delta.path)
        : []
    const legacyShape: DirPatchEvent = {
      tabId: event.tabId,
      path: event.path,
      reason: 'watch',
      changed,
      removed,
    }
    get().applyDirPatch(legacyShape)
  },
  setSort: async (paneId, sortKey) => {
    // A streamed listing owns the entries until its terminal chunk. Rust's
    // tail remains ordered for the sort requested by its head, so accepting a
    // local non-Items sort here would corrupt the accumulated row order too.
    if (get().panes[paneId].loading || !get().panes[paneId].listingComplete) {
      return
    }
    if (isTrashPath(get().panes[paneId].path)) {
      set((state) => {
        const pane = state.panes[paneId]
        const sortDirection =
          pane.sortKey === sortKey
            ? pane.sortDirection === 'asc'
              ? 'desc'
              : 'asc'
            : sortKey === 'name' || sortKey === 'type'
              ? 'asc'
              : 'desc'
        return {
          panes: {
            ...state.panes,
            [paneId]: {
              ...pane,
              sortKey,
              sortDirection,
              entries: sortEntries(pane.entries, sortKey, sortDirection, state.sizeStates),
              itemsSortStatus: 'complete',
            },
          },
        }
      })
      return
    }
    let shouldRunActiveSort = false
    set((state) => {
      const pane = state.panes[paneId]
      const sortDirection =
        pane.sortKey === sortKey
          ? pane.sortDirection === 'asc'
            ? 'desc'
            : 'asc'
          : sortKey === 'name' || sortKey === 'type'
            ? 'asc'
            : 'desc'
      const isItemsSort = sortKey === 'items'
      shouldRunActiveSort = isItemsSort
      const sortedEntries = isItemsSort
        ? pane.entries
        : sortEntries(pane.entries, sortKey, sortDirection, state.sizeStates)

      return {
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            entries: sortedEntries,
            sortKey,
            sortDirection,
            itemsSortStatus: isItemsSort ? 'counting' : 'idle',
          },
        },
      }
    })

    const nextPane = get().panes[paneId]
    useTabsStore.getState().patchActiveTab(paneId, {
      sortKey: nextPane.sortKey,
      sortDirection: nextPane.sortDirection,
    })
    schedulePersistSession(get().activePaneId)
    if (shouldRunActiveSort) {
      await runActiveItemsSort(paneId, currentItemCountContext(paneId, nextPane), {
        filterApplied: nextPane.filterApplied,
        sortDirection: nextPane.sortDirection,
      })
    }
  },
  setFilterDraft: (paneId, value) => {
    const timer = get().filterTimers[paneId]
    if (timer) {
      window.clearTimeout(timer)
    }

    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          filterDraft: value,
          typing: true,
        },
      },
    }))

    const nextTimer = window.setTimeout(() => {
      set((state) => ({
        panes: {
          ...state.panes,
          [paneId]: {
            ...state.panes[paneId],
            filterApplied: value,
            typing: false,
            itemsSortStatus:
              state.panes[paneId].sortKey === 'items'
                ? 'counting'
                : state.panes[paneId].itemsSortStatus,
          },
        },
      }))
      void get().reloadPane(paneId)
    }, filterDelayMs)

    set((state) => ({
      filterTimers: {
        ...state.filterTimers,
        [paneId]: nextTimer,
      },
    }))
  },
  clearFilter: (paneId) => {
    const timer = get().filterTimers[paneId]
    if (timer) {
      window.clearTimeout(timer)
    }

    set((state) => ({
      filterTimers: {
        ...state.filterTimers,
        [paneId]: undefined,
      },
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          filterDraft: '',
          filterApplied: '',
          typing: false,
          itemsSortStatus:
            state.panes[paneId].sortKey === 'items'
              ? 'counting'
              : state.panes[paneId].itemsSortStatus,
        },
      },
    }))

    void get().reloadPane(paneId)
  },
  setFocusedEntry: (paneId, entryId) =>
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          focusedEntryId: entryId,
        },
      },
    })),
  setScrollPosition: (paneId, path, scrollTop) => {
    const previous = get().panes[paneId].scrollPositions[path]
    if (previous === scrollTop) {
      return
    }

    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          scrollPositions: {
            ...state.panes[paneId].scrollPositions,
            [path]: scrollTop,
          },
        },
      },
    }))
  },
  applySizeStates: (events) =>
    set((state) => {
      if (events.length === 0) {
        return {}
      }

      const nextSizeStates = { ...state.sizeStates }
      const terminalPaths: string[] = []
      let calculatingCount = 0
      for (const event of events) {
        nextSizeStates[event.path] = {
          state: event.state,
          sizeBytes: event.sizeBytes,
          source: event.source,
        }
        if (event.state === 'calculating') {
          calculatingCount += 1
        }
        if (isTerminalSizeState(event.state)) {
          terminalPaths.push(event.path)
        }
      }

      const previousBucket = Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL)
      sizeEventsProcessed += events.length
      if (Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL) > previousBucket) {
        log.debug('size: events applied', {
          totalProcessed: sizeEventsProcessed,
          lastBatch: events.length,
          calculatingInBatch: calculatingCount,
          terminalInBatch: terminalPaths.length,
        })
      }

      const protectedSizePaths = new Set<string>()
      for (const pane of Object.values(state.panes)) {
        for (const entry of pane.entries) {
          if (entry.id === pane.focusedEntryId) protectedSizePaths.add(entry.path)
        }
      }

      return {
        pendingSizeRequests:
          terminalPaths.length > 0
            ? withPendingSizeRequests(state.pendingSizeRequests, terminalPaths, false)
            : state.pendingSizeRequests,
        sizeStates: pruneSizeCache(nextSizeStates, protectedSizePaths),
      }
    }),
  requestVisibleItemCounts: async (paneId, visibleStartIndex, visibleEndIndex, viewportCount) => {
    const pane = get().panes[paneId]
    if (
      pane.loading ||
      isTrashPath(pane.path) ||
      pane.itemsSortStatus === 'counting' ||
      pane.listRequestId === 0 ||
      pane.entries.length === 0
    ) {
      return
    }

    const context = currentItemCountContext(paneId, pane)
    const overscan = Math.max(0, viewportCount)
    const startIndex = Math.max(0, visibleStartIndex - overscan)
    const endIndex = Math.min(pane.entries.length - 1, visibleEndIndex + overscan)
    if (endIndex < startIndex) {
      return
    }

    const pending = (() => {
      const nextPaths: string[] = []
      const seen = new Set<string>()
      const passiveItemCountRequests = get().passiveItemCountRequests
      let remaining = AUTO_VISIBLE_ITEM_COUNT_LIMIT
      for (const key of Object.keys(passiveItemCountRequests)) {
        if (key.startsWith(`${itemCountContextKey(context)}::`)) {
          remaining -= 1
        }
      }

      if (remaining <= 0) {
        return nextPaths
      }

      for (let index = startIndex; index <= endIndex; index += 1) {
        const entry = pane.entries[index]
        if (!entry?.isDir || entry.itemCount != null) {
          continue
        }

        const normalizedPath = pathKey(entry.path)
        if (seen.has(normalizedPath)) {
          continue
        }
        seen.add(normalizedPath)

        const requestKey = itemCountRequestKey(context, entry.path)
        if (passiveItemCountRequests[requestKey]) {
          continue
        }

        nextPaths.push(entry.path)
        if (nextPaths.length >= remaining) {
          break
        }
      }

      return nextPaths
    })()

    if (pending.length === 0) {
      return
    }

    set((state) => {
      const currentPane = state.panes[paneId]
      if (!itemCountContextMatches(paneId, currentPane, context)) {
        return state
      }

      // The backend supersedes automatic work for this scope. Keep dedupe
      // aligned with its latest request, otherwise paths cancelled by a fast
      // scroll can remain marked pending forever with a null count.
      const nextPassiveItemCountRequests = Object.fromEntries(
        Object.entries(state.passiveItemCountRequests).filter(
          ([key]) => !key.startsWith(`${itemCountContextKey(context)}::`),
        ),
      ) as PassiveItemCountRequests
      for (const path of pending) {
        nextPassiveItemCountRequests[itemCountRequestKey(context, path)] = true
      }

      return { passiveItemCountRequests: nextPassiveItemCountRequests }
    })

    try {
      await requestVisibleItemCountsCommand({ context, paths: pending })
    } catch (error) {
      set((state) => {
        const nextPassiveItemCountRequests = { ...state.passiveItemCountRequests }
        for (const path of pending) {
          delete nextPassiveItemCountRequests[itemCountRequestKey(context, path)]
        }
        return { passiveItemCountRequests: nextPassiveItemCountRequests }
      })
      throw error
    }
  },
  applyItemCountEvents: (events) =>
    set((state) => {
      if (events.length === 0) {
        return state
      }

      let panesChanged = false
      const nextPanes = { ...state.panes }
      for (const paneId of Object.keys(nextPanes) as PaneId[]) {
        const pane = nextPanes[paneId]
        let nextEntries: DirectoryEntry[] | undefined

        for (const event of events) {
          if (!itemCountContextMatches(paneId, pane, event.context)) {
            continue
          }

          for (const result of event.results) {
            const entryIndex = pane.entries.findIndex(
              (entry) => entry.isDir && pathsMatch(entry.path, result.path),
            )
            if (entryIndex < 0) {
              continue
            }

            const currentEntries = nextEntries ?? pane.entries
            if (currentEntries[entryIndex].itemCount === result.itemCount) {
              continue
            }

            if (!nextEntries) {
              nextEntries = [...pane.entries]
            }

            nextEntries[entryIndex] = {
              ...nextEntries[entryIndex],
              itemCount: result.itemCount,
            }
          }
        }

        if (nextEntries) {
          nextPanes[paneId] = { ...pane, entries: nextEntries }
          panesChanged = true
        }
      }

      return panesChanged ? { panes: nextPanes } : state
    }),
  requestVisibleIcons: async (paneId, paths) => {
    const pane = get().panes[paneId]
    const pending = collectPendingIconRequests(
      pane.entries,
      paths,
      get().pendingIconRequests,
      get().resolvedIconPaths,
    )

    if (pending.length === 0) {
      return
    }

    set((state) => ({
      pendingIconRequests: withPendingIconRequests(state.pendingIconRequests, pending, true),
    }))

    try {
      await requestIcons({ paths: pending })
    } catch (error) {
      set((state) => ({
        pendingIconRequests: withPendingIconRequests(state.pendingIconRequests, pending, false),
      }))
      throw error
    }
  },
  applyIconStates: (events) =>
    set((state) => {
      if (events.length === 0) {
        return {}
      }

      // Only allocate a new `panes` object/entries array for panes that
      // actually changed. If no buffered path matched any entry in either
      // pane, `panes` stays the exact same reference as `state.panes` so the
      // update is a true no-op re-render-wise (FR7).
      let panesChanged = false
      const nextPanes = { ...state.panes }
      for (const paneId of Object.keys(nextPanes) as PaneId[]) {
        const pane = nextPanes[paneId]
        let nextEntries: DirectoryEntry[] | undefined

        for (const event of events) {
          const entryIndex = pane.entries.findIndex((entry) => entry.path === event.path)
          if (entryIndex < 0) {
            continue
          }

          const currentEntries = nextEntries ?? pane.entries
          if (currentEntries[entryIndex].iconDataUrl === event.iconDataUrl) {
            continue
          }

          if (!nextEntries) {
            nextEntries = [...pane.entries]
          }
          nextEntries[entryIndex] = {
            ...nextEntries[entryIndex],
            iconDataUrl: event.iconDataUrl,
          }
        }

        if (nextEntries) {
          nextPanes[paneId] = { ...pane, entries: nextEntries }
          panesChanged = true
        }
      }

      const resolvedIconPaths = { ...state.resolvedIconPaths }
      const iconCacheTouched = { ...state.iconCacheTouched }
      const now = iconCacheNow()
      const paths: string[] = []
      for (const event of events) {
        paths.push(event.path)
        // Recorded even when `iconDataUrl` is null: a resolved-to-nothing
        // result must stop future requests just as much as a real icon does.
        resolvedIconPaths[event.path] = event.iconDataUrl
        iconCacheTouched[event.path] = now
      }

      const protectedIconPaths = new Set<string>()
      for (const pane of Object.values(state.panes)) {
        for (const entry of pane.entries) protectedIconPaths.add(entry.path)
      }
      const iconRecords = Object.fromEntries(
        Object.entries(resolvedIconPaths).map(([path, value]) => [
          path,
          { value, touched: iconCacheTouched[path] ?? now, weight: iconWeight(value) },
        ]),
      )
      const retainedIcons = pruneIconCache(
        iconRecords,
        protectedIconPaths,
        now,
        typeof navigator !== 'undefined' && navigator.platform.startsWith('Win'),
      )
      const boundedResolvedIconPaths = Object.fromEntries(
        Object.entries(retainedIcons).map(([path, record]) => [path, record.value]),
      ) as ResolvedIconPaths
      const boundedIconCacheTouched = Object.fromEntries(
        Object.keys(retainedIcons).map((path) => [path, iconCacheTouched[path] ?? now]),
      ) as Record<string, number>

      return {
        panes: panesChanged ? nextPanes : state.panes,
        pendingIconRequests: withPendingIconRequests(state.pendingIconRequests, paths, false),
        resolvedIconPaths: boundedResolvedIconPaths,
        iconCacheTouched: boundedIconCacheTouched,
      }
    }),
  setEverythingStatus: (everythingStatus) => set({ everythingStatus }),
  setVolumes: (volumes) => {
    treeLoadGeneration += 1
    return set((state) => {
      const nextTree = buildRootTreeState(volumes, state.treeNodes)
      return {
        volumes,
        treeRoots: nextTree.treeRoots,
        treeNodes: pruneTreeCache(nextTree.treeNodes as Record<string, LazyTreeNode>, [
          state.panes.left.path,
          state.panes.right.path,
          ...nextTree.treeRoots,
        ]) as Record<string, TreeNodeState>,
      }
    })
  },
  setShowHiddenFiles: async (showHiddenFiles) => {
    if (get().showHiddenFiles === showHiddenFiles) {
      return
    }

    // The pane store drives `list_dir(show_hidden)`; the config store owns
    // persistence. Keep both in sync, persist once (via the config store), then
    // reload both panes so the new visibility takes effect immediately.
    set({ showHiddenFiles })
    await useConfigStore.getState().setShowHiddenFiles(showHiddenFiles)
    await Promise.all([get().reloadPane('left'), get().reloadPane('right')])
  },
  requestManualSize: async (paneId, entryId) => {
    const pane = get().panes[paneId]
    const entry = pane.entries.find((item) => item.id === entryId)
    if (!entry?.isDir) {
      return
    }

    await requestFolderSize({ path: entry.path })
  },
  calculateAllFolderSizes: async (paneId) => {
    const pane = get().panes[paneId]
    const paths = pane.entries.filter((entry) => entry.isDir).map((entry) => entry.path)
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

    set((state) => ({
      pendingSizeRequests: withPendingSizeRequests(state.pendingSizeRequests, pending, true),
    }))

    const startedAt = performance.now()
    try {
      await requestFolderSizes({ paths: pending })
      log.debug('size: calculateAllFolderSizes request returned', {
        paneId,
        requested: pending.length,
        ms: Math.round(performance.now() - startedAt),
      })
    } catch (error) {
      set((state) => ({
        pendingSizeRequests: withPendingSizeRequests(state.pendingSizeRequests, pending, false),
      }))
      throw error
    }
  },
  ensureTreeChildren: async (path) => {
    if (!path) {
      return
    }

    const currentNode = get().treeNodes[path]
    if (currentNode?.loaded) {
      return
    }

    const existingFlight = treeLoadFlights.get(path)
    if (existingFlight) {
      return existingFlight
    }

    let settleFlight: () => void = () => undefined
    const flight = new Promise<void>((resolve) => {
      settleFlight = resolve
    })
    treeLoadFlights.set(path, flight)
    const requestGeneration = treeLoadGeneration

    try {
      const response = await listTreeChildren({
        path,
        showHidden: get().showHiddenFiles,
      })

      const directoryChildren = response.children
      set((state) => ({
        // A reset/volume replacement superseded this request. Do not revive a
        // retired subtree from a late IPC response.
        ...(requestGeneration !== treeLoadGeneration
          ? {}
          : {
              treeNodes: {
                ...state.treeNodes,
                [path]: {
                  id: path,
                  name: currentNode?.name ?? basenameFromPath(path),
                  path,
                  parentPath: treeRootForPath(state.treeRoots, path)
                    ? null
                    : (currentNode?.parentPath ?? getParentPath(path)),
                  children: directoryChildren.map(
                    (entry) => treeRootForPath(state.treeRoots, entry.path) ?? entry.path,
                  ),
                  // Newly-seen volume roots should open on first hydration just like
                  // the initial roots. Once a root has loaded, preserve the user's
                  // explicit expanded/collapsed choice on later refreshes.
                  expanded:
                    currentNode == null
                      ? true
                      : currentNode.expanded ||
                        (currentNode.parentPath === null && !currentNode.loaded),
                  loaded: true,
                  // This request is the only evidence needed for the node
                  // itself: no child directory has been probed.
                  expandability: directoryChildren.length === 0 ? 'empty' : 'nonEmpty',
                  lastAccess: Date.now(),
                },
                // Always refresh name/parentPath from this fresh listing (the source
                // of truth) rather than trusting an existing node, which may be a
                // placeholder created before its parent was ever listed (e.g. a pane
                // navigated straight to it) and so was named from its raw path.
                ...Object.fromEntries(
                  directoryChildren.map((entry) => {
                    const volumeRoot = treeRootForPath(state.treeRoots, entry.path)
                    const nodePath = volumeRoot ?? entry.path
                    const existing = state.treeNodes[nodePath] ?? state.treeNodes[entry.path]
                    return [
                      nodePath,
                      existing
                        ? {
                            ...existing,
                            name: volumeRoot ? existing.name : entry.name,
                            parentPath: volumeRoot ? null : path,
                          }
                        : {
                            id: nodePath,
                            name: entry.name,
                            path: nodePath,
                            parentPath: volumeRoot ? null : path,
                            children: [],
                            expanded: false,
                            loaded: false,
                            expandability: expandabilityOf(entry),
                            lastAccess: Date.now(),
                          },
                    ]
                  }),
                ),
              },
            }),
      }))
    } catch {
      set((state) => ({
        ...(requestGeneration !== treeLoadGeneration
          ? {}
          : {
              treeNodes: {
                ...state.treeNodes,
                [path]: {
                  id: path,
                  name: currentNode?.name ?? basenameFromPath(path),
                  path,
                  parentPath: treeRootForPath(state.treeRoots, path)
                    ? null
                    : (currentNode?.parentPath ?? getParentPath(path)),
                  children: [],
                  expanded:
                    currentNode == null
                      ? false
                      : currentNode.expanded ||
                        (currentNode.parentPath === null && !currentNode.loaded),
                  loaded: true,
                  expandability: currentNode?.expandability ?? 'empty',
                  lastAccess: Date.now(),
                },
              },
            }),
      }))
    } finally {
      // Retention is enforced after every completed direct-child load, not
      // only when volume topology changes. Active pane paths and roots are
      // protected; all other least-recent nodes are evictable.
      set((state) => ({
        treeNodes: pruneTreeCache(state.treeNodes as Record<string, LazyTreeNode>, [
          state.panes.left.path,
          state.panes.right.path,
          ...state.treeRoots,
        ]) as Record<string, TreeNodeState>,
      }))
      treeLoadFlights.delete(path)
      settleFlight()
    }
  },
  revealPath: async (path) => {
    if (!path) {
      return
    }

    // Only reveal paths that live under one of the tree roots (volumes); paths
    // outside any volume have no node to expand to.
    const root = findVolumeForPath(path, get().volumes)?.mountRoot ?? null
    if (!root) {
      return
    }

    // Build the ancestor chain from the root down to the active path.
    const chain: string[] = []
    let current: string | null = path
    while (current) {
      chain.unshift(current)
      if (pathsMatch(current, root)) {
        break
      }
      current = getParentPath(current)
    }

    // Load each ancestor's children so the next node down the chain exists, then
    // expand every ancestor (all but the active leaf) so the active node renders.
    for (let index = 0; index < chain.length - 1; index += 1) {
      await get().ensureTreeChildren(chain[index])
    }

    set((state) => {
      // Ancestors of the active path (all chain entries but the leaf) stay open;
      // every other expanded branch collapses so only the active path is shown.
      const ancestors = new Set(chain.slice(0, -1).map((entry) => pathKey(entry)))
      const leaf = pathKey(path)
      const treeNodes = { ...state.treeNodes }
      for (const [nodePath, node] of Object.entries(state.treeNodes)) {
        const nodePathKey = pathKey(nodePath)
        if (ancestors.has(nodePathKey)) {
          if (!node.expanded) {
            treeNodes[nodePath] = { ...node, expanded: true }
          }
        } else if (nodePathKey !== leaf && node.expanded) {
          treeNodes[nodePath] = { ...node, expanded: false }
        }
      }
      return { treeNodes }
    })
  },
  toggleTreeNode: async (path) => {
    const node = get().treeNodes[path]
    if (!node?.loaded) {
      await get().ensureTreeChildren(path)
    }

    set((state) => ({
      treeNodes: {
        ...state.treeNodes,
        [path]: {
          ...(state.treeNodes[path] ?? {
            id: path,
            name: basenameFromPath(path),
            path,
            parentPath: getParentPath(path),
            children: [],
            loaded: false,
            expandability: 'unknown',
            lastAccess: Date.now(),
          }),
          expanded: !state.treeNodes[path]?.expanded,
        },
      },
    }))
  },
  reset: () => {
    treeLoadGeneration += 1
    treeLoadFlights.clear()
    delete pendingParentRevealPaths.left
    delete pendingParentRevealPaths.right
    useTabsStore.getState().reset()
    // `release()` resets each session's baseline/collection synchronously
    // before its own (best-effort) async IPC call, so this clears in-memory
    // v2 session state immediately even though the call isn't awaited here.
    void listingSessions.left.release()
    void listingSessions.right.release()
    set(defaultState())
  },
}))

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
