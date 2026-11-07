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
  ItemCountRequestContext,
  SessionPatchEvent,
  SessionState,
  SessionViewParams,
  SizeStateEvent,
  SortDirection,
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
import {
  normalizeWindowsCompatibilityPath,
  pathKey,
  pathsMatch,
  samePathOrWindowsCaseFold,
} from '@/lib/path-compare'
import {
  findVolumeForPath,
  formatVolumeTreeName,
  isPathInsideVolume,
  sortVolumesForTree,
} from '@/lib/volumes'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { useSelectionStore } from '@/stores/selection-store'
import { ListingSession } from '@/stores/panes/listing-session'
import { expandabilityOf, pruneTreeCache, type LazyTreeNode } from '@/stores/panes/tree-cache'
import { pruneIconCache, pruneSizeCache } from '@/stores/panes/cache-policy'
import type { PaneId } from '@/types/pane'
import { otherPane } from '@/lib/pane-id'
import { lookupOwnProperty, withKeyFlags } from '@/lib/collections'
import { cacheNow } from '@/stores/weighted-cache'
import { utf16ByteWeight } from '@/lib/numbers'

type SizeStateKind = SizeStateEvent['state']

// When `viaHistory` is set the navigation is a back/forward step and must not
// rewrite the pane's history stack (the index has already been moved).
type NavigateOptions = {
  viaHistory?: boolean
  /** An entry to select and scroll to once the listing resolves. */
  revealPath?: string
}

export type TreeFoldReveals = {
  /** `pathKey` of the leaf these reveals belong to. */
  leaf: string | null
  revealedSiblings: string[]
  showHiddenLevels: boolean
}

export type TreeNodeState = Omit<LazyTreeNode, 'expandability' | 'lastAccess'> & {
  expandability?: LazyTreeNode['expandability']
  lastAccess?: number
}

type EntrySizeState = {
  state: SizeStateKind
  sizeBytes: number | null
  source: SizeStateEvent['source']
  naReason?: SizeStateEvent['naReason']
  isLowerBound?: boolean
  skippedRoots?: string[]
  skippedMore?: number
  /** A `ready` value kept visible while a refresh recalculates it. */
  refreshing?: boolean
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
  folderSizeIndexStatus: FolderSizeIndexStatus
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
  folderSizeIndexStatus: FolderSizeIndexStatus | null
  volumes: VolumeInfo[]
  sizeStates: Record<string, EntrySizeState>
  pendingSizeRequests: PendingSizeRequests
  pendingIconRequests: PendingIconRequests
  passiveItemCountRequests: PassiveItemCountRequests
  resolvedIconPaths: ResolvedIconPaths
  iconCacheTouched: Record<string, number>
  treeNodes: Record<string, TreeNodeState>
  treeRoots: string[]
  // Which folds of the active path the user opened in the tree. Keyed to the
  // revealed leaf so navigating elsewhere folds the tree back up.
  treeFold: TreeFoldReveals
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
  openTabFromPath: (
    paneId: PaneId,
    path: string,
    options?: { revealPath?: string },
  ) => Promise<void>
  closeTab: (paneId: PaneId, tabId: string) => Promise<void>
  /** Closes every unlocked tab on one side of `tabId`, keeping `tabId` itself open. */
  closeTabsBeside: (paneId: PaneId, tabId: string, side: 'left' | 'right') => Promise<void>
  setTabLocked: (paneId: PaneId, tabId: string, locked: boolean) => void
  switchTab: (paneId: PaneId, tabId: string) => Promise<void>
  moveTab: (
    sourcePaneId: PaneId,
    tabId: string,
    destinationPaneId: PaneId,
    destinationIndex: number,
  ) => Promise<TabMoveResult>
  /** Refresh / F5: reloads the pane and re-requests its folder sizes in `refresh` mode, keeping known values visible. */
  refreshSizes: (paneId: PaneId) => Promise<void>
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
   * either pane, so a no-op icon update never triggers a re-render.
   */
  applyIconStates: (events: IconStateEvent[]) => void
  setFolderSizeIndexStatus: (status: FolderSizeIndexStatus) => void
  setVolumes: (volumes: VolumeInfo[]) => void
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  requestManualSize: (paneId: PaneId, entryId: string) => Promise<void>
  calculateAllFolderSizes: (paneId: PaneId) => Promise<void>
  ensureTreeChildren: (path: string | null) => Promise<void>
  revealPath: (path: string | null) => Promise<void>
  toggleTreeNode: (path: string) => Promise<void>
  /** Shows every child of `parentPath` instead of its "N other folders" row. */
  revealTreeSiblings: (parentPath: string) => void
  /** Shows the middle levels folded into the "N levels hidden" row. */
  revealTreeHiddenLevels: () => void
  reset: () => void
}

const filterDelayMs = 180
const AUTO_VISIBLE_ITEM_COUNT_LIMIT = 200
const STALE_SIZE_LOG_SAMPLE_LIMIT = 3
// Each path has one owner while its direct children are loading. Callers share
// that promise, preventing double expands/reveals from issuing duplicate IPC.
const treeLoadFlights = new Map<string, Promise<void>>()
let treeLoadGeneration = 0

// An entry to select and reveal once a pane's next listing resolves: the
// folder being left when a navigation moves to its direct parent (Up/Backspace,
// the synthetic ".." row, or a history step that lands one level above), or an
// explicit `revealPath` (a Search everywhere hit). Keyed per pane and consumed
// (cleared) by the next `reloadPane` for that pane, so a superseding
// navigation or an unrelated reload never applies a stale reveal.
const pendingRevealPaths: Partial<Record<PaneId, string>> = {}
let revealScrollRequestCounter = 0

// ---------------------------------------------------------------------------
// v2 directory-session integration.
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
// The full listing is materialized eagerly (walking every v2 page until
// `loadedRowCount === totalRows`, see `ensureSessionFullyLoaded` below)
// because `FilePane`, marquee, drag-and-drop and keyboard navigation read the
// flat array rather than a sparse `PaneEntryCollection`. Rust's session
// already holds the fully sorted listing in memory (paging only slices an
// already-materialized vector), so walking every page does no extra
// enumeration or sorting work.
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
const replacementViewTokens: Record<PaneId, number> = { left: 0, right: 0 }

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
 * Walks every page of `session`'s active collection until
 * every row is loaded, pinning every page for the duration so the 12-page LRU
 * retention in `PaneEntryCollection` cannot evict an earlier page while a
 * later one is still being fetched (a real risk for directories over 6000
 * entries, since only the viewport +/- 1 page is pinned by default). Once
 * fully loaded, pins are reset back to just the viewport (page 0). Resolves
 * early if the session's baseline moves on (a newer navigation/view revision
 * superseded this walk) so a stale materialization request can never hang.
 */
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
    folderSizeIndexStatus: null as FolderSizeIndexStatus | null,
    volumes: [] as VolumeInfo[],
    sizeStates: {} as Record<string, EntrySizeState>,
    pendingSizeRequests: {} as PendingSizeRequests,
    pendingIconRequests: {} as PendingIconRequests,
    passiveItemCountRequests: {} as PassiveItemCountRequests,
    resolvedIconPaths: {} as ResolvedIconPaths,
    iconCacheTouched: {} as Record<string, number>,
    treeNodes: {} as Record<string, TreeNodeState>,
    treeRoots: [] as string[],
    treeFold: { leaf: null, revealedSiblings: [], showHiddenLevels: false } as TreeFoldReveals,
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

  path = normalizeWindowsCompatibilityPath(path)

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
  const normalized = normalizeWindowsCompatibilityPath(path)
  const stripped = normalized.replace(/[\\/]+$/, '')
  const lastSep = Math.max(stripped.lastIndexOf('\\'), stripped.lastIndexOf('/'))
  if (lastSep < 0 || lastSep === stripped.length - 1) {
    return stripped || normalized
  }
  return stripped.slice(lastSep + 1)
}

type EntrySortValue = (
  entry: DirectoryEntry,
  sizeState: EntrySizeState | undefined,
) => string | number

const entrySortValues: Record<SortKey, EntrySortValue> = {
  name: (entry) => entry.name.toLowerCase(),
  size: (entry, sizeState) => sizeState?.sizeBytes ?? entry.sizeBytes ?? -1,
  items: (entry) => entry.itemCount ?? -1,
  type: (entry) => entry.typeLabel.toLowerCase(),
  modified: (entry) => entry.modifiedAt ?? '',
  created: (entry) => entry.createdAt ?? '',
}

function entrySortValue(
  entry: DirectoryEntry,
  sizeState: EntrySizeState | undefined,
  sortKey: SortKey,
) {
  const sortValue = lookupOwnProperty(entrySortValues, sortKey) ?? entrySortValues.name
  return sortValue(entry, sizeState)
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

  const sizeOrder = sortKey === 'size' ? compareSizeKnowledge(left, right, sizeStates) : null
  if (sizeOrder !== null) {
    return sizeOrder
  }

  const direction = sortDirection === 'asc' ? 1 : -1
  const valueOrder = compareSortValues(
    entrySortValue(left, sizeStates[left.path], sortKey),
    entrySortValue(right, sizeStates[right.path], sortKey),
  )
  const order = valueOrder || compareEntryNames(left, right)
  return order * direction
}

function compareSizeKnowledge(
  left: DirectoryEntry,
  right: DirectoryEntry,
  sizeStates: Record<string, EntrySizeState>,
): number | null {
  const leftHasKnownSize = isKnownSize(left, sizeStates[left.path])
  const rightHasKnownSize = isKnownSize(right, sizeStates[right.path])
  if (leftHasKnownSize !== rightHasKnownSize) {
    return leftHasKnownSize ? -1 : 1
  }
  return leftHasKnownSize ? null : compareEntryNames(left, right)
}

function compareSortValues(left: string | number, right: string | number) {
  if (left < right) {
    return -1
  }
  return left > right ? 1 : 0
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

const ITEM_COUNT_KEY_SEPARATOR = '::'

function itemCountContextKey(context: ItemCountRequestContext) {
  return [context.paneId, context.tabId, context.requestId].join(ITEM_COUNT_KEY_SEPARATOR)
}

function itemCountRequestKey(context: ItemCountRequestContext, path: string) {
  return `${itemCountContextKey(context)}${ITEM_COUNT_KEY_SEPARATOR}${pathKey(path)}`
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

// Eager folder-size requests are only valid where a folder-size index can
// answer them cheaply (Everything on Windows, the macOS index). Without one,
// sizes are strictly manual (Space / "Calculate size"). The request is sent in
// `auto` mode, so on macOS Rust answers paths outside the index `unknown`
// without walking them. Network paths are handled by the Rust size layer
// (always N/A); we never pre-gate on those here.
function eagerSizesEnabled(status: FolderSizeIndexStatus | null, autoFolderSize: boolean): boolean {
  return (status?.isAvailable ?? false) && autoFolderSize
}

/** Replaces (or clears) a pane's live size-update set; failures only log. */
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
    nextTreeNodes[volume.mountRoot] = volumeRootNode(volume, currentNodes[volume.mountRoot])
  }

  return { treeRoots, treeNodes: nextTreeNodes }
}

function volumeRootNode(volume: VolumeInfo, existing: TreeNodeState | undefined): TreeNodeState {
  return {
    id: volume.mountRoot,
    name: formatVolumeTreeName(volume),
    path: volume.mountRoot,
    parentPath: null,
    ...retainedRootNodeState(existing),
  }
}

function retainedRootNodeState(existing: TreeNodeState | undefined) {
  if (!existing) {
    return {
      children: [],
      expanded: false,
      loaded: false,
      expandability: 'unknown' as const,
      lastAccess: Date.now(),
    }
  }
  return {
    children: existing.children,
    expanded: existing.expanded,
    loaded: existing.loaded,
    expandability: existing.expandability ?? 'unknown',
    lastAccess: existing.lastAccess ?? Date.now(),
  }
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
function affectedPatchPaths(event: DirPatchEvent) {
  const affected = [...event.removed]
  for (const change of event.changed) {
    if (!affected.some((path) => pathsMatch(path, change.path))) {
      affected.push(change.path)
    }
  }
  return affected
}

/**
 * Reconcile the changed set by path (last write wins, matching the previous
 * map-based behaviour); a change whose entry is missing or no longer matches
 * the pane's filter/hidden rules stays dropped rather than re-inserted.
 */
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

type PatchMerge = (
  kept: DirectoryEntry[],
  inserted: DirectoryEntry[],
  pane: PaneState,
  sizeStates: Record<string, EntrySizeState>,
) => DirectoryEntry[]

const insertIntoSortedEntries: PatchMerge = (kept, inserted, pane, sizeStates) => {
  for (const entry of inserted) {
    insertSorted(kept, entry, pane.sortKey, pane.sortDirection, sizeStates)
  }
  return kept
}

// Size ordering depends on folder sizes that resolve asynchronously and are not
// continuously re-sorted into the listing, so the array can't be assumed sorted
// against the current sizeStates — fall back to a full re-sort for that one key.
// Active items sorting also waits for the backend-owned count-then-sort pass, so
// watcher patches preserve the current local order plus changed rows rather than
// finalizing a client-side items order.
const patchMergeBySortKey: Partial<Record<SortKey, PatchMerge>> = {
  size: (kept, inserted, pane, sizeStates) =>
    sortEntries([...kept, ...inserted], pane.sortKey, pane.sortDirection, sizeStates),
  items: (kept, inserted) => [...kept, ...inserted],
}

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

  const affected = affectedPatchPaths(event)
  const kept =
    affected.length > 0
      ? pane.entries.filter((entry) => !affected.some((path) => pathsMatch(path, entry.path)))
      : [...pane.entries]
  const inserted = patchInserts(event, pane, showHiddenFiles).map((entry) =>
    hydrateEntryIcon(entry, resolvedIconPaths),
  )
  const merge = patchMergeBySortKey[pane.sortKey] ?? insertIntoSortedEntries
  return merge(kept, inserted, pane, sizeStates)
}

function entryMatchesPane(entry: DirectoryEntry, pane: PaneState, showHiddenFiles: boolean) {
  if (!showHiddenFiles && (entry.isHidden || entry.isSystem)) {
    return false
  }

  return (
    pane.filterApplied === '' || entry.name.toLowerCase().includes(pane.filterApplied.toLowerCase())
  )
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

  return Object.keys(treeNodes).find((nodePath) => pathsMatch(nodePath, path)) ?? null
}

function childPathInList(children: string[], path: string) {
  return children.find((childPath) => pathsMatch(childPath, path)) ?? null
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

function treeNodeAtPath(treeNodes: Record<string, TreeNodeState>, path: string) {
  const nodeKey = treeNodeKeyForPath(treeNodes, path)
  return nodeKey ? treeNodes[nodeKey] : undefined
}

function visibleDirectory(entry: DirectoryEntry | null, showHiddenFiles: boolean) {
  if (!entry?.isDir) {
    return null
  }
  const hiddenFromTree = !showHiddenFiles && (entry.isHidden || entry.isSystem)
  return hiddenFromTree ? null : entry
}

function inheritedExpandability(
  previous: TreeNodeState | undefined,
  existing: TreeNodeState | undefined,
) {
  return previous?.expandability ?? existing?.expandability ?? 'unknown'
}

function inheritedTreeNodeState(
  previous: TreeNodeState | undefined,
  existing: TreeNodeState | undefined,
) {
  const source = previous ?? existing
  return {
    children: source?.children ?? [],
    expanded: source?.expanded ?? false,
    loaded: source?.loaded ?? false,
    expandability: inheritedExpandability(previous, existing),
  }
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
      nextChildren = children.filter((candidate) => !pathsMatch(candidate, path))
    }

    if (!pruneTree) {
      return
    }

    const nodeKey = treeNodeKeyForPath(nextTreeNodes, path)
    if (!nodeKey) {
      return
    }

    const node = nextTreeNodes[nodeKey]
    if (!pathsMatch(node.parentPath ?? '', parent.path) && !childPath) {
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

  const upsertDirectory = (entry: DirectoryEntry, previous: TreeNodeState | undefined) => {
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
      ...inheritedTreeNodeState(previous, existing),
      lastAccess: Date.now(),
    }

    if (!childPathInList(nextChildren, entry.path)) {
      mutableChildren().push(entry.path)
    }
  }

  for (const change of event.changed) {
    const directory = visibleDirectory(change.entry, state.showHiddenFiles)
    const previous = treeNodeAtPath(nextTreeNodes, change.path)
    removeChild(change.path, directory === null)
    if (directory) {
      upsertDirectory(directory, previous)
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

type ListingRequestContext = Parameters<typeof matchesListingContext>[1]

type ListingAttempt = {
  paneId: PaneId
  tabId: string
  listingGeneration: number
  requestContext: ListingRequestContext
}

function isListingAttemptStale(attempt: ListingAttempt) {
  const currentPane = usePanesStore.getState().panes[attempt.paneId]
  return (
    activeTab(attempt.paneId).id !== attempt.tabId ||
    currentPane.listingGeneration !== attempt.listingGeneration ||
    !matchesListingContext(currentPane, attempt.requestContext)
  )
}

/**
 * A path change (or the pane/tab's very first listing) begins a fresh session;
 * an unchanged path with a different sort/filter/hidden view revises the
 * already-active session instead.
 */
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

function isMaterializedListingStale(attempt: ListingAttempt, session: ListingSession) {
  return isListingAttemptStale(attempt) || session.currentBaseline == null
}

/**
 * Navigating up one level should land with the folder we came from selected
 * and focused, matching Explorer/xplorer2; an explicit reveal (a search hit)
 * always wins.
 */
function revealTargetAfterNavigation(
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

/** Parks the reveal target for `reloadPane` to resolve against the fresh listing. */
function parkRevealPath(paneId: PaneId, revealTarget: string | undefined) {
  if (revealTarget) {
    pendingRevealPaths[paneId] = revealTarget
    return
  }
  delete pendingRevealPaths[paneId]
}

function throwIfSessionFailed(session: ListingSession) {
  if (session.currentStatus === 'error') {
    throw new Error(session.currentError ?? 'Failed to load directory')
  }
}

/**
 * Rust is the canonical sort authority for every key the `SessionViewParams`
 * contract knows about, but folder sizes resolve asynchronously after the
 * listing (and are not a `SessionViewParams` field), so the size key re-sorts
 * the fully-materialized entries on the client. Items sort is unaffected — it
 * already runs as a separate backend count-then-sort pass via
 * `runActiveItemsSort`, which replaces `pane.entries` wholesale once ready.
 */
function clientSortedEntries(
  loadedEntries: DirectoryEntry[],
  pane: PaneState,
  sizeStates: Record<string, EntrySizeState>,
) {
  if (pane.sortKey !== 'size') {
    return loadedEntries
  }
  return sortEntries(loadedEntries, pane.sortKey, pane.sortDirection, sizeStates)
}

function findRevealEntry(entries: DirectoryEntry[], revealPath: string | undefined) {
  if (!revealPath) {
    return undefined
  }
  return entries.find((entry) => pathsMatch(entry.path, revealPath))
}

/**
 * The trash browser is a virtual location backed by list_trash, not a real
 * directory: no fs watcher, tree-children lookup, or size requests apply to it.
 */
async function reloadTrashPane(
  paneId: PaneId,
  pane: PaneState,
  selection: ReturnType<typeof useSelectionStore.getState>['selections'][PaneId],
) {
  const response = await listTrash()
  const sortedEntries = sortEntries(
    response.entries.map(trashEntryToDirectoryEntry),
    pane.sortKey,
    pane.sortDirection,
    usePanesStore.getState().sizeStates,
  )

  usePanesStore.setState((state) => {
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

  const nextTrashPane = usePanesStore.getState().panes[paneId]
  useTabsStore.getState().patchActiveTab(paneId, {
    path: nextTrashPane.path,
    sortKey: nextTrashPane.sortKey,
    sortDirection: nextTrashPane.sortDirection,
    filter: nextTrashPane.filterApplied,
  })
  schedulePersistSession(usePanesStore.getState().activePaneId)
  // Trash shows no folder sizes, so the pane's live size set is dropped.
  await reportSizeSubscriptions(paneId, null, [])
}

function logDirPatchPathMismatch(paneId: PaneId, pane: PaneState, event: DirPatchEvent) {
  log.debug('applyDirPatch skipped (path mismatch)', {
    paneId,
    panePath: pane.path,
    eventPath: event.path,
  })
}

function treeNodesUpdate(treeNodes: Record<string, TreeNodeState> | null) {
  return treeNodes ? { treeNodes } : {}
}

function retainedFocusId(entries: DirectoryEntry[], focusedEntryId: string | null) {
  if (entries.some((entry) => entry.id === focusedEntryId)) {
    return focusedEntryId
  }
  return entries[0]?.id ?? null
}

function withoutPatchedItemCountRequests(
  state: PanesStore,
  paneId: PaneId,
  pane: PaneState,
  event: DirPatchEvent,
) {
  const context = currentItemCountContext(paneId, pane)
  const invalidatedPaths = new Set(
    event.changed
      .filter(invalidatesItemCount)
      .map((change) => itemCountRequestKey(context, change.path)),
  )
  if (invalidatedPaths.size === 0) {
    return state.passiveItemCountRequests
  }
  return Object.fromEntries(
    Object.entries(state.passiveItemCountRequests).filter(([key]) => !invalidatedPaths.has(key)),
  ) as PassiveItemCountRequests
}

function invalidatesItemCount(change: DirPatchEvent['changed'][number]) {
  const entry = change.entry
  if (!entry?.isDir) {
    return false
  }
  return entry.itemCount == null
}

/**
 * Items sorting is backend-owned count-then-sort. Preserve the accepted rows
 * verbatim until that replacement arrives; locally removing or inserting patch
 * rows would show an untrustworthy transient order.
 */
function applyItemsSortedDirPatch(state: PanesStore, paneId: PaneId, event: DirPatchEvent) {
  const pane = state.panes[paneId]
  if (!pathsMatch(pane.path, event.path)) {
    logDirPatchPathMismatch(paneId, pane, event)
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
    panes: { ...state.panes, [paneId]: { ...pane, itemsSortStatus: 'counting' as const } },
    ...treeNodesUpdate(treeNodes),
  }
}

/** A recalculation never blanks a known value: keep it and mark it refreshing until the new result lands. */
function nextEntrySizeState(
  previous: EntrySizeState | undefined,
  event: SizeStateEvent,
): EntrySizeState {
  if (event.state !== 'calculating') {
    return freshEntrySizeState(event)
  }
  if (previous?.state !== 'ready') {
    return freshEntrySizeState(event)
  }
  return { ...previous, refreshing: true }
}

function freshEntrySizeState(event: SizeStateEvent): EntrySizeState {
  return {
    state: event.state,
    sizeBytes: event.sizeBytes,
    source: event.source,
    naReason: event.naReason,
    isLowerBound: event.isLowerBound,
    skippedRoots: event.skippedRoots,
    skippedMore: event.skippedMore,
  }
}

type SizeEventBatchSummary = { batchSize: number; calculatingCount: number; terminalCount: number }

function logSizeEventProgress({
  batchSize,
  calculatingCount,
  terminalCount,
}: SizeEventBatchSummary) {
  const previousBucket = Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL)
  sizeEventsProcessed += batchSize
  if (Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL) > previousBucket) {
    log.debug('size: events applied', {
      totalProcessed: sizeEventsProcessed,
      lastBatch: batchSize,
      calculatingInBatch: calculatingCount,
      terminalInBatch: terminalCount,
    })
  }
}

function focusedEntryPath(pane: PaneState) {
  return pane.entries.find((entry) => entry.id === pane.focusedEntryId)?.path
}

function focusedEntryPaths(panes: Record<PaneId, PaneState>) {
  const paths = new Set<string>()
  for (const pane of Object.values(panes)) {
    const path = focusedEntryPath(pane)
    if (path !== undefined) {
      paths.add(path)
    }
  }
  return paths
}

function allEntryPaths(panes: Record<PaneId, PaneState>) {
  return new Set(Object.values(panes).flatMap((pane) => pane.entries.map((entry) => entry.path)))
}

function canCountVisibleItems(pane: PaneState) {
  if (pane.loading) {
    return false
  }
  if (isTrashPath(pane.path)) {
    return false
  }
  if (pane.itemsSortStatus === 'counting') {
    return false
  }
  if (pane.listRequestId === 0) {
    return false
  }
  return pane.entries.length > 0
}

function needsVisibleItemCount(entry: DirectoryEntry) {
  return entry.isDir && entry.itemCount == null
}

function visibleItemCountPaths({
  pane,
  context,
  startIndex,
  endIndex,
  passiveItemCountRequests,
}: {
  pane: PaneState
  context: ItemCountRequestContext
  startIndex: number
  endIndex: number
  passiveItemCountRequests: PassiveItemCountRequests
}) {
  const contextPrefix = `${itemCountContextKey(context)}${ITEM_COUNT_KEY_SEPARATOR}`
  const alreadyRequested = Object.keys(passiveItemCountRequests).filter((key) =>
    key.startsWith(contextPrefix),
  ).length
  const remaining = Math.max(0, AUTO_VISIBLE_ITEM_COUNT_LIMIT - alreadyRequested)
  const visibleEntries = pane.entries.slice(startIndex, endIndex + 1)
  const uncountedFolders = firstByPathKey(visibleEntries.filter(needsVisibleItemCount))
  const notYetRequested = uncountedFolders.filter(
    (entry) => !passiveItemCountRequests[itemCountRequestKey(context, entry.path)],
  )
  const withinBudget = notYetRequested.slice(0, remaining)
  return withinBudget.map((entry) => entry.path)
}

function firstByPathKey(entries: DirectoryEntry[]) {
  const seen = new Set<string>()
  return entries.filter((entry) => {
    const key = pathKey(entry.path)
    if (seen.has(key)) {
      return false
    }
    seen.add(key)
    return true
  })
}

function updateEntries<Update>(
  entries: DirectoryEntry[],
  updates: Update[],
  locate: (update: Update) => number,
  apply: (entry: DirectoryEntry, update: Update) => DirectoryEntry | null,
): DirectoryEntry[] | null {
  let nextEntries: DirectoryEntry[] | null = null
  for (const update of updates) {
    const index = locate(update)
    const updated = index < 0 ? null : apply((nextEntries ?? entries)[index], update)
    if (!updated) {
      continue
    }
    nextEntries ??= [...entries]
    nextEntries[index] = updated
  }
  return nextEntries
}

/**
 * Only allocates a new `panes` object for panes whose entries changed; `null`
 * keeps `state.panes` the exact same reference so the update is a true no-op
 * re-render-wise.
 */
function patchPaneEntries(
  panes: Record<PaneId, PaneState>,
  patchPane: (paneId: PaneId, pane: PaneState) => DirectoryEntry[] | null,
): Record<PaneId, PaneState> | null {
  let changed = false
  const nextPanes = { ...panes }
  for (const paneId of Object.keys(panes) as PaneId[]) {
    const entries = patchPane(paneId, panes[paneId])
    if (entries) {
      nextPanes[paneId] = { ...panes[paneId], entries }
      changed = true
    }
  }
  return changed ? nextPanes : null
}

function isDirectoryAtPath(entry: DirectoryEntry, path: string) {
  if (!entry.isDir) {
    return false
  }
  return pathsMatch(entry.path, path)
}

function entriesWithItemCounts(paneId: PaneId, pane: PaneState, events: ItemCountEvent[]) {
  const results = events
    .filter((event) => itemCountContextMatches(paneId, pane, event.context))
    .flatMap((event) => event.results)
  return updateEntries(
    pane.entries,
    results,
    (result) => pane.entries.findIndex((entry) => isDirectoryAtPath(entry, result.path)),
    (entry, result) =>
      entry.itemCount === result.itemCount ? null : { ...entry, itemCount: result.itemCount },
  )
}

function entriesWithIcons(pane: PaneState, events: IconStateEvent[]) {
  return updateEntries(
    pane.entries,
    events,
    (event) => pane.entries.findIndex((entry) => entry.path === event.path),
    (entry, event) =>
      entry.iconDataUrl === event.iconDataUrl ? null : { ...entry, iconDataUrl: event.iconDataUrl },
  )
}

function isWindowsNavigator() {
  return typeof navigator !== 'undefined' && navigator.platform.startsWith('Win')
}

function boundedIconCache(
  resolvedIconPaths: ResolvedIconPaths,
  iconCacheTouched: Record<string, number>,
  protectedIconPaths: Set<string>,
  now: number,
) {
  const iconRecords = Object.fromEntries(
    Object.entries(resolvedIconPaths).map(([path, value]) => [
      path,
      { value, touched: iconCacheTouched[path] ?? now, weight: utf16ByteWeight(value) },
    ]),
  )
  const retainedIcons = pruneIconCache(iconRecords, protectedIconPaths, now, isWindowsNavigator())
  return {
    resolvedIconPaths: Object.fromEntries(
      Object.entries(retainedIcons).map(([path, record]) => [path, record.value]),
    ) as ResolvedIconPaths,
    iconCacheTouched: Object.fromEntries(
      Object.keys(retainedIcons).map((path) => [path, iconCacheTouched[path] ?? now]),
    ) as Record<string, number>,
  }
}

type TreeChildren = Awaited<ReturnType<typeof listTreeChildren>>['children']

/** A reset/volume replacement superseded the request; a late IPC response must not revive a retired subtree. */
function isTreeLoadSuperseded(requestGeneration: number) {
  return requestGeneration !== treeLoadGeneration
}

function loadedNodeIdentity(
  path: string,
  currentNode: TreeNodeState | undefined,
  treeRoots: string[],
) {
  const isRoot = Boolean(treeRootForPath(treeRoots, path))
  return {
    id: path,
    name: currentNode?.name ?? basenameFromPath(path),
    path,
    parentPath: isRoot ? null : (currentNode?.parentPath ?? getParentPath(path)),
  }
}

/**
 * Newly-seen volume roots open on first hydration just like the initial roots.
 * Once a root has loaded, the user's explicit expanded/collapsed choice holds.
 */
function expandedAfterLoad(currentNode: TreeNodeState | undefined, expandNewNode: boolean) {
  if (currentNode == null) {
    return expandNewNode
  }
  const unloadedRoot = currentNode.parentPath === null && !currentNode.loaded
  return currentNode.expanded || unloadedRoot
}

/**
 * Always refresh name/parentPath from the fresh listing (the source of truth)
 * rather than trusting an existing node, which may be a placeholder created
 * before its parent was ever listed (e.g. a pane navigated straight to it) and
 * so was named from its raw path.
 */
type ListedChildNode = { nodePath: string; node: TreeNodeState }

function listedChildNode(
  state: PanesStore,
  parentPath: string,
  entry: TreeChildren[number],
): ListedChildNode {
  const volumeRoot = treeRootForPath(state.treeRoots, entry.path)
  const nodePath = volumeRoot ?? entry.path
  const existing = state.treeNodes[nodePath] ?? state.treeNodes[entry.path]
  const listedParentPath = volumeRoot ? null : parentPath
  if (existing) {
    const name = volumeRoot ? existing.name : entry.name
    return { nodePath, node: { ...existing, name, parentPath: listedParentPath } }
  }
  return {
    nodePath,
    node: {
      id: nodePath,
      name: entry.name,
      path: nodePath,
      parentPath: listedParentPath,
      children: [],
      expanded: false,
      loaded: false,
      expandability: expandabilityOf(entry),
      lastAccess: Date.now(),
    },
  }
}

function listedChildNodes(state: PanesStore, parentPath: string, directoryChildren: TreeChildren) {
  const nodes: Record<string, TreeNodeState> = {}
  for (const entry of directoryChildren) {
    const listed = listedChildNode(state, parentPath, entry)
    nodes[listed.nodePath] = listed.node
  }
  return nodes
}

function loadedTreeNodes(
  state: PanesStore,
  path: string,
  currentNode: TreeNodeState | undefined,
  directoryChildren: TreeChildren,
): Record<string, TreeNodeState> {
  return {
    ...state.treeNodes,
    [path]: {
      ...loadedNodeIdentity(path, currentNode, state.treeRoots),
      children: directoryChildren.map(
        (entry) => treeRootForPath(state.treeRoots, entry.path) ?? entry.path,
      ),
      expanded: expandedAfterLoad(currentNode, true),
      loaded: true,
      // This request is the only evidence needed for the node itself: no
      // child directory has been probed.
      expandability: directoryChildren.length === 0 ? 'empty' : 'nonEmpty',
      lastAccess: Date.now(),
    },
    ...listedChildNodes(state, path, directoryChildren),
  }
}

function failedTreeNodes(
  state: PanesStore,
  path: string,
  currentNode: TreeNodeState | undefined,
): Record<string, TreeNodeState> {
  return {
    ...state.treeNodes,
    [path]: {
      ...loadedNodeIdentity(path, currentNode, state.treeRoots),
      children: [],
      expanded: expandedAfterLoad(currentNode, false),
      loaded: true,
      expandability: currentNode?.expandability ?? 'empty',
      lastAccess: Date.now(),
    },
  }
}

export const usePanesStore = create<PanesStore>((set, get) => ({
  ...defaultState(),
  initialize: ({ session, showHiddenFiles, folderSizeIndexStatus, volumes }) => {
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
    useRecentFoldersStore.getState().hydrate(session.recentFolders ?? [])

    const activeLeft = activeTab('left')
    const activeRight = activeTab('right')
    const { treeRoots, treeNodes } = buildRootTreeState(volumes)

    set((state) => ({
      ...state,
      activePaneId: session.activePane === 'right' ? 'right' : 'left',
      showHiddenFiles,
      // A `size://index-status` event that arrived before this initial
      // fetch resolved is newer; keep it.
      folderSizeIndexStatus: state.folderSizeIndexStatus ?? folderSizeIndexStatus,
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
    const revealChildPath = pendingRevealPaths[paneId]
    delete pendingRevealPaths[paneId]
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
    const attempt: ListingAttempt = {
      paneId,
      tabId,
      listingGeneration: get().panes[paneId].listingGeneration,
      requestContext,
    }

    try {
      if (isTrashPath(pane.path)) {
        await reloadTrashPane(paneId, pane, selection)
        return
      }

      // v2 seekable directory session: Rust enumerates + sorts the whole
      // listing once on `begin_directory_session` and holds it server-side;
      // the frontend walks every page eagerly via `ensureSessionFullyLoaded`.
      const session = listingSessions[paneId]
      const view = currentViewParams(pane, get().showHiddenFiles)
      if (revisesActiveSession(session, paneId, tabId, pane.path)) {
        await session.reviseView(view)
      } else {
        await session.begin(tabId, pane.path, view)
      }

      if (isListingAttemptStale(attempt)) {
        return
      }
      throwIfSessionFailed(session)

      await ensureSessionFullyLoaded(paneId, session)

      // Re-check staleness after the (possibly multi-page) materialization
      // walk: a newer navigation/sort/filter may have superseded this one
      // while pages were still loading.
      if (isMaterializedListingStale(attempt, session)) {
        return
      }

      const resolvedPath = session.currentPath ?? pane.path
      const sortedEntries = clientSortedEntries(session.loadedEntries(), pane, get().sizeStates)

      // A monotonic identity for this successful listing, matching the old
      // `head.requestId` semantics: `finalizeListing`/`requestVisibleItemCounts`
      // compare it to reject stale item-count/watch-seed responses.
      listRequestIdCounter += 1
      const requestId = listRequestIdCounter

      // The reveal target (the folder we came up from, or a search hit) wins
      // over the generic focus restore below, and also becomes the selection
      // so the row is visibly highlighted, not just focused.
      const revealEntry = findRevealEntry(sortedEntries, revealChildPath)
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
      // Trash listings return earlier, so only real folders are recorded.
      useRecentFoldersStore.getState().record(nextPane.path)
      schedulePersistSession(get().activePaneId)

      await runActiveItemsSort(paneId, currentItemCountContext(paneId, nextPane), {
        filterApplied: nextPane.filterApplied,
        sortDirection: nextPane.sortDirection,
      })
      await get().finalizeListing(paneId)
    } catch (error) {
      if (isListingAttemptStale(attempt)) {
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
      await get().openTabFromPath(paneId, path, { revealPath: options.revealPath })
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

    parkRevealPath(paneId, revealTargetAfterNavigation(leaving.path, path, options.revealPath))

    const otherPaneId = otherPane(paneId)
    const otherDirPaths = new Set(directoryPaths(get().panes[otherPaneId].entries))
    const leavingDirPaths = directoryPaths(leaving.entries)
    const staleSizePaths = leavingDirPaths.filter(
      (candidate) =>
        !otherDirPaths.has(candidate) &&
        (get().pendingSizeRequests[candidate] ||
          get().sizeStates[candidate]?.state === 'calculating' ||
          get().sizeStates[candidate]?.refreshing === true),
    )
    log.debug('size: navigate cancel evaluation', {
      paneId,
      from: leaving.path,
      to: path,
      leavingDirs: leavingDirPaths.length,
      stale: staleSizePaths.length,
      sampleStale: staleSizePaths.slice(0, STALE_SIZE_LOG_SAMPLE_LIMIT),
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
          } else if (sizeStates[stalePath]?.refreshing) {
            sizeStates[stalePath] = { ...sizeStates[stalePath], refreshing: false }
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
            elapsedMilliseconds: Math.round(performance.now() - cancelStartedAt),
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
  openTabFromPath: async (paneId, path, options = {}) => {
    log.debug('openTabFromPath', { paneId, path })
    if (options.revealPath) {
      pendingRevealPaths[paneId] = options.revealPath
    } else {
      delete pendingRevealPaths[paneId]
    }
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

    // The closed tab owned the pane's live size set; the next tab's listing
    // reports its own.
    void reportSizeSubscriptions(paneId, null, [])

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
  closeTabsBeside: async (paneId, tabId, side) => {
    const { tabs, activeTabIndex } = useTabsStore.getState().panes[paneId]
    const index = tabs.findIndex((tab) => tab.id === tabId)
    if (index === -1) {
      return
    }

    const closing = (side === 'left' ? tabs.slice(0, index) : tabs.slice(index + 1)).filter(
      (tab) => !tab.locked,
    )
    // Activate the kept tab first so the closes below never reload the pane.
    if (closing.some((tab) => tab.id === tabs[activeTabIndex]?.id)) {
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

    const resetPane = (paneId: PaneId) => {
      const tab = activeTab(paneId)
      set((state) => ({
        passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
          state.passiveItemCountRequests,
          paneId,
        ),
        panes: {
          ...state.panes,
          [paneId]: {
            ...state.panes[paneId],
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
            historyIndex: -1,
          },
        },
      }))
    }

    resetPane(result.destinationPaneId)
    if (result.sourceActiveChanged) {
      resetPane(result.sourcePaneId)
    }
    set({ activePaneId: result.destinationPaneId })

    // Replace sessions before the fresh reloads, so late data remains tied to
    // the superseded tab IDs and is rejected by the existing guards.
    void listingSessions[result.destinationPaneId].release()
    if (result.sourceActiveChanged) {
      void listingSessions[result.sourcePaneId].release()
    }
    await Promise.all([
      get().reloadPane(result.destinationPaneId),
      ...(result.sourceActiveChanged ? [get().reloadPane(result.sourcePaneId)] : []),
    ])
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
  refreshSizes: async (paneId) => {
    // `revise_directory_session_view` re-derives sort/filter from Rust's
    // already-cached enumeration snapshot without touching the filesystem
    // again — wrong for an explicit "refresh from disk" action. Releasing the
    // session first forces `reloadPane`'s same-path check to fall through to
    // `begin_directory_session`, which re-enumerates.
    await listingSessions[paneId].release()
    await get().reloadPane(paneId)

    // Known values stay visible until replaced: instead of clearing
    // them, re-request in `refresh` mode, bypassing the dedup that skips
    // `ready` folders. Without automatic sizing only folders that already
    // have a result are recalculated.
    const pane = get().panes[paneId]
    if (isTrashPath(pane.path)) {
      return
    }
    const folders = directoryPaths(pane.entries)
    const eager =
      eagerSizesEnabled(get().folderSizeIndexStatus, useConfigStore.getState().autoFolderSize) &&
      folders.length <= AUTO_FOLDER_SIZE_MAX_DIRECTORIES
    const paths = eager
      ? folders
      : folders.filter((path) => {
          const state = get().sizeStates[path]?.state
          return state !== undefined && isTerminalSizeState(state)
        })
    if (paths.length === 0) {
      return
    }

    set((state) => ({
      pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, paths, true),
    }))
    try {
      await requestFolderSizes({ paths, mode: 'refresh' })
    } catch (error) {
      set((state) => ({
        pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, paths, false),
      }))
      throw error
    }
  },
  finalizeListing: async (paneId) => {
    const pane = get().panes[paneId]
    // Trash is a virtual location with no fs watcher / tree / size requests.
    if (isTrashPath(pane.path)) {
      return
    }

    const eager = () =>
      eagerSizesEnabled(get().folderSizeIndexStatus, useConfigStore.getState().autoFolderSize)

    const requestDirectorySizes = async (paths: string[]) => {
      if (!eager()) {
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
        pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, pending, true),
      }))

      try {
        await requestFolderSizes({ paths: pending, mode: 'auto' })
        return pending
      } catch (error) {
        set((state) => ({
          pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, pending, false),
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

      // Report the live size set before requesting sizes, so a protected
      // folder this pane just opened counts as visited for those requests.
      // The set is sent even without automatic sizing, so manually
      // calculated indexed sizes still update live.
      const folders = directoryPaths(get().panes[paneId].entries)
      await reportSizeSubscriptions(
        paneId,
        pane.path,
        folders.slice(0, AUTO_FOLDER_SIZE_MAX_DIRECTORIES),
      )
      const requested = await requestDirectorySizes(folders)

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
  // `dir://patch` watcher events splice the affected rows into
  // `pane.entries` in place (see `patchEntries` below) instead of re-running
  // `ListingSession.begin` on every watch tick. The `ListingSession`'s
  // `PaneEntryCollection` is left un-patched (it only supports full-page
  // `installPage` replaces) and is re-materialized the next time `reloadPane`
  // runs, so it can only trail `pane.entries` between a watch patch and the
  // pane's next reload.
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

      if (pane.sortKey === 'items') {
        return applyItemsSortedDirPatch(state, paneId, event)
      }

      const entries = patchEntries(
        pane,
        event,
        state.sizeStates,
        state.showHiddenFiles,
        state.resolvedIconPaths,
      )
      if (!entries) {
        logDirPatchPathMismatch(paneId, pane, event)
        return state
      }

      log.debug('applyDirPatch applied', {
        paneId,
        path: event.path,
        entries: entries.length,
        changed: event.changed.length,
        removed: event.removed.length,
      })

      return {
        passiveItemCountRequests: withoutPatchedItemCountRequests(state, paneId, pane, event),
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            entries,
            itemsSortStatus: pane.itemsSortStatus,
            focusedEntryId: retainedFocusId(entries, pane.focusedEntryId),
          },
        },
        ...treeNodesUpdate(applyTreeDirPatch(state, event)),
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
      replacementViewTokens[paneId] += 1
      const replacementToken = replacementViewTokens[paneId]
      void (async () => {
        try {
          let acceptedBaseline = event.nextBaseline
          while (replacementViewTokens[paneId] === replacementToken) {
            await ensureSessionFullyLoaded(paneId, session)
            if (
              activeTab(paneId).id !== event.tabId ||
              !pathsMatch(get().panes[paneId].path, event.path)
            ) {
              return
            }
            if (session.currentBaseline !== acceptedBaseline) {
              if (!session.currentBaseline) return
              acceptedBaseline = session.currentBaseline
              continue
            }
            break
          }
          if (replacementViewTokens[paneId] !== replacementToken) return

          const loadedEntries = session.loadedEntries()
          listRequestIdCounter += 1
          const requestId = listRequestIdCounter
          set((state) => {
            const currentPane = state.panes[paneId]
            if (
              session.currentBaseline !== acceptedBaseline ||
              activeTab(paneId).id !== event.tabId ||
              !pathsMatch(currentPane.path, event.path)
            ) {
              return state
            }

            const entries = withResolvedIcons(
              currentPane.sortKey === 'size'
                ? sortEntries(
                    loadedEntries,
                    currentPane.sortKey,
                    currentPane.sortDirection,
                    state.sizeStates,
                  )
                : loadedEntries,
              state.resolvedIconPaths,
            )
            const entryPaths = new Set(entries.map((entry) => pathKey(entry.path)))
            const treeEvent: DirPatchEvent = {
              tabId: event.tabId,
              path: event.path,
              reason: 'watch',
              changed: entries
                .filter((entry) => entry.isDir)
                .map((entry) => ({ path: entry.path, entry })),
              removed: currentPane.entries
                .filter((entry) => entry.isDir && !entryPaths.has(pathKey(entry.path)))
                .map((entry) => entry.path),
            }
            const focusedStillPresent = entries.some(
              (entry) => entry.id === currentPane.focusedEntryId,
            )
            const treeNodes = applyTreeDirPatch(state, treeEvent)

            return {
              passiveItemCountRequests: clearPassiveItemCountRequestsForPane(
                state.passiveItemCountRequests,
                paneId,
              ),
              panes: {
                ...state.panes,
                [paneId]: {
                  ...currentPane,
                  entries,
                  listRequestId: requestId,
                  focusedEntryId: focusedStillPresent
                    ? currentPane.focusedEntryId
                    : (entries[0]?.id ?? null),
                },
              },
              ...(treeNodes ? { treeNodes } : {}),
            }
          })

          const currentPane = get().panes[paneId]
          void runActiveItemsSort(paneId, currentItemCountContext(paneId, currentPane), {
            filterApplied: currentPane.filterApplied,
            sortDirection: currentPane.sortDirection,
          })
        } catch (error) {
          log.error('applySessionPatch failed to materialize replacement view', {
            paneId,
            path: event.path,
            error,
          })
        }
      })()
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
        nextSizeStates[event.path] = nextEntrySizeState(nextSizeStates[event.path], event)
        if (event.state === 'calculating') {
          calculatingCount += 1
        }
        if (isTerminalSizeState(event.state)) {
          terminalPaths.push(event.path)
        }
      }
      logSizeEventProgress({
        batchSize: events.length,
        calculatingCount,
        terminalCount: terminalPaths.length,
      })

      return {
        pendingSizeRequests:
          terminalPaths.length > 0
            ? withKeyFlags(state.pendingSizeRequests, terminalPaths, false)
            : state.pendingSizeRequests,
        sizeStates: pruneSizeCache(nextSizeStates, focusedEntryPaths(state.panes)),
      }
    }),
  requestVisibleItemCounts: async (paneId, visibleStartIndex, visibleEndIndex, viewportCount) => {
    const pane = get().panes[paneId]
    if (!canCountVisibleItems(pane)) {
      return
    }

    const context = currentItemCountContext(paneId, pane)
    const overscan = Math.max(0, viewportCount)
    const startIndex = Math.max(0, visibleStartIndex - overscan)
    const endIndex = Math.min(pane.entries.length - 1, visibleEndIndex + overscan)
    if (endIndex < startIndex) {
      return
    }

    const pending = visibleItemCountPaths({
      pane,
      context,
      startIndex,
      endIndex,
      passiveItemCountRequests: get().passiveItemCountRequests,
    })

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
          ([key]) => !key.startsWith(`${itemCountContextKey(context)}${ITEM_COUNT_KEY_SEPARATOR}`),
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
      const panes = patchPaneEntries(state.panes, (paneId, pane) =>
        entriesWithItemCounts(paneId, pane, events),
      )
      return panes ? { panes } : state
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
      pendingIconRequests: withKeyFlags(state.pendingIconRequests, pending, true),
    }))

    try {
      await requestIcons({ paths: pending })
    } catch (error) {
      set((state) => ({
        pendingIconRequests: withKeyFlags(state.pendingIconRequests, pending, false),
      }))
      throw error
    }
  },
  applyIconStates: (events) =>
    set((state) => {
      if (events.length === 0) {
        return {}
      }

      const panes = patchPaneEntries(state.panes, (_paneId, pane) => entriesWithIcons(pane, events))

      const resolvedIconPaths = { ...state.resolvedIconPaths }
      const iconCacheTouched = { ...state.iconCacheTouched }
      const now = cacheNow()
      const paths: string[] = []
      for (const event of events) {
        paths.push(event.path)
        // Recorded even when `iconDataUrl` is null: a resolved-to-nothing
        // result must stop future requests just as much as a real icon does.
        resolvedIconPaths[event.path] = event.iconDataUrl
        iconCacheTouched[event.path] = now
      }

      return {
        panes: panes ?? state.panes,
        pendingIconRequests: withKeyFlags(state.pendingIconRequests, paths, false),
        ...boundedIconCache(resolvedIconPaths, iconCacheTouched, allEntryPaths(state.panes), now),
      }
    }),
  setFolderSizeIndexStatus: (folderSizeIndexStatus) => set({ folderSizeIndexStatus }),
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
      pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, pending, true),
    }))

    const startedAt = performance.now()
    try {
      await requestFolderSizes({ paths: pending })
      log.debug('size: calculateAllFolderSizes request returned', {
        paneId,
        requested: pending.length,
        elapsedMilliseconds: Math.round(performance.now() - startedAt),
      })
    } catch (error) {
      set((state) => ({
        pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, pending, false),
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
      set((state) =>
        isTreeLoadSuperseded(requestGeneration)
          ? {}
          : { treeNodes: loadedTreeNodes(state, path, currentNode, directoryChildren) },
      )
    } catch {
      set((state) =>
        isTreeLoadSuperseded(requestGeneration)
          ? {}
          : { treeNodes: failedTreeNodes(state, path, currentNode) },
      )
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
      const treeFold =
        state.treeFold.leaf === leaf
          ? state.treeFold
          : { leaf, revealedSiblings: [], showHiddenLevels: false }
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
      return { treeNodes, treeFold }
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
  revealTreeSiblings: (parentPath) => {
    set((state) =>
      state.treeFold.revealedSiblings.includes(parentPath)
        ? state
        : {
            treeFold: {
              ...state.treeFold,
              revealedSiblings: [...state.treeFold.revealedSiblings, parentPath],
            },
          },
    )
  },
  revealTreeHiddenLevels: () => {
    set((state) => ({ treeFold: { ...state.treeFold, showHiddenLevels: true } }))
  },
  reset: () => {
    treeLoadGeneration += 1
    treeLoadFlights.clear()
    delete pendingRevealPaths.left
    delete pendingRevealPaths.right
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
