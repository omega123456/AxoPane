import { create } from 'zustand'
import {
  cancelSizes,
  listTreeChildren,
  startListDir,
  listTrash,
  requestFolderSize,
  requestFolderSizes,
  requestIcons,
  saveSession,
  setTabWatch,
} from '@/lib/ipc/commands'
import { isTrashPath, trashEntryToDirectoryEntry } from '@/lib/trash'
import type {
  DirectoryEntry,
  DirPatchEvent,
  EverythingStatus,
  IconStateEvent,
  ListChunkEvent,
  SessionState,
  SizeStateEvent,
  SortDirection,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import { activeTab, fromSessionPane, toSessionPane, useTabsStore } from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { log } from '@/lib/app-log-commands'
import { formatVolumeTreeName, isPathInsideVolume, sortVolumesForTree } from '@/lib/volumes'
import { useSelectionStore } from '@/stores/selection-store'

type PaneId = 'left' | 'right'
type SizeStateKind = SizeStateEvent['state']

// When `viaHistory` is set the navigation is a back/forward step and must not
// rewrite the pane's history stack (the index has already been moved).
type NavigateOptions = { viaHistory?: boolean }

export type TreeNodeState = {
  id: string
  name: string
  path: string
  parentPath: string | null
  children: string[]
  expanded: boolean
  loaded: boolean
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
  error: string | null
  // Monotonic id of the in-flight streamed listing for this pane's active tab,
  // set from the `start_list_dir` head. Streamed `list_chunk` events are only
  // applied while their `requestId` matches, so chunks from a superseded
  // navigation are ignored.
  listRequestId: number
  scrollPositions: Record<string, number>
  history: string[]
  historyIndex: number
}

type PendingSizeRequests = Record<string, true>
type PendingIconRequests = Record<string, true>
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
  resolvedIconPaths: ResolvedIconPaths
  treeNodes: Record<string, TreeNodeState>
  treeRoots: string[]
  filterTimers: Partial<Record<PaneId, number>>
  initialize: (payload: InitializePayload) => void
  setActivePane: (paneId: PaneId) => void
  reloadPane: (paneId: PaneId) => Promise<void>
  navigatePane: (paneId: PaneId, path: string, options?: NavigateOptions) => Promise<void>
  goUp: (paneId: PaneId) => Promise<void>
  goBack: (paneId: PaneId) => Promise<void>
  goForward: (paneId: PaneId) => Promise<void>
  openTabFromPath: (paneId: PaneId, path: string) => Promise<void>
  closeTab: (paneId: PaneId, tabId: string) => Promise<void>
  switchTab: (paneId: PaneId, tabId: string) => Promise<void>
  refreshEverything: (paneId: PaneId) => Promise<void>
  /** Appends a batch of streamed listing chunks to their pane, ignoring superseded ones. */
  applyListChunk: (events: ListChunkEvent[]) => void
  /** Runs the post-listing tail (arm fs-watch, eager folder sizes, tree children) once a listing is complete. */
  finalizeListing: (paneId: PaneId) => Promise<void>
  applyDirPatch: (event: DirPatchEvent) => void
  setSort: (paneId: PaneId, sortKey: SortKey) => Promise<void>
  setFilterDraft: (paneId: PaneId, value: string) => void
  clearFilter: (paneId: PaneId) => void
  setFocusedEntry: (paneId: PaneId, entryId: string | null) => void
  setScrollPosition: (paneId: PaneId, path: string, scrollTop: number) => void
  /** Applies a batch of size-state events (coalesced by the frontend rAF batcher) in one `set`. */
  applySizeStates: (events: SizeStateEvent[]) => void
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
    error: null,
    listRequestId: 0,
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
    resolvedIconPaths: {} as ResolvedIconPaths,
    treeNodes: {} as Record<string, TreeNodeState>,
    treeRoots: [] as string[],
    filterTimers: {} as Partial<Record<PaneId, number>>,
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
  const activeRoots = treeRoots.map((root) => root.toLowerCase())
  const nextTreeNodes = Object.fromEntries(
    Object.entries(currentNodes).filter(([, node]) => {
      if (node.parentPath === null) {
        return false
      }

      const nodePath = node.path.toLowerCase()
      return activeRoots.some((root) => isPathInsideVolume(nodePath, root))
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
    }
  }

  return { treeRoots, treeNodes: nextTreeNodes }
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

  const affected = new Set<string>(event.removed)
  for (const change of event.changed) {
    affected.add(change.path)
  }

  const next =
    affected.size > 0
      ? pane.entries.filter((entry) => !affected.has(entry.path))
      : [...pane.entries]

  // Reconcile the changed set by path (last write wins, matching the previous
  // map-based behaviour); a change whose entry is missing or no longer matches
  // the pane's filter/hidden rules stays dropped rather than re-inserted.
  const inserts = new Map<string, DirectoryEntry>()
  for (const change of event.changed) {
    if (change.entry && entryMatchesPane(change.entry, pane, showHiddenFiles)) {
      inserts.set(change.entry.path, change.entry)
    } else {
      inserts.delete(change.path)
    }
  }

  const hydrated = Array.from(inserts.values(), (entry) =>
    hydrateEntryIcon(entry, resolvedIconPaths),
  )

  // Size ordering depends on folder sizes that resolve asynchronously and are
  // not continuously re-sorted into the listing, so the array can't be assumed
  // sorted against the current sizeStates — fall back to a full re-sort for
  // that one key. Every other key is a pure function of entry fields, so the
  // listing stays sorted and each change can be binary-inserted in place.
  if (pane.sortKey === 'size') {
    return sortEntries([...next, ...hydrated], pane.sortKey, pane.sortDirection, sizeStates)
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
  return left === right || left.toLowerCase() === right.toLowerCase()
}

function treeNodeKeyForPath(treeNodes: Record<string, TreeNodeState>, path: string) {
  if (treeNodes[path]) {
    return path
  }

  const normalized = path.toLowerCase()
  return Object.keys(treeNodes).find((nodePath) => nodePath.toLowerCase() === normalized) ?? null
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
      tabs: [{ path: session.leftPath || '.', sortKey: 'name', sortDirection: 'asc', filter: '' }],
    }
    const rightPane = session.right ?? {
      activeTabIndex: 0,
      tabs: [{ path: session.rightPath || '.', sortKey: 'name', sortDirection: 'asc', filter: '' }],
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
    const selection = useSelectionStore.getState().selections[paneId]
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: { ...state.panes[paneId], loading: true, error: null },
      },
    }))

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

      const itemsColumnVisible =
        useLayoutStore.getState().columns.find((column) => column.key === 'items')?.visible ?? true

      // Stream the listing: Rust enumerates + sorts on its worker thread and
      // returns only the inline first chunk here; any remaining entries arrive
      // as `list_chunk` events (see `applyListChunk`). This keeps the webview
      // from parsing a single multi-MB listing array in one blocking task.
      const head = await startListDir({
        tabId: activeTab(paneId).id,
        path: pane.path,
        sortKey: pane.sortKey,
        sortDirection: pane.sortDirection,
        filter: pane.filterApplied,
        showHidden: get().showHiddenFiles,
        includeItemCounts: itemsColumnVisible,
      })

      // Rust is the canonical sort authority, so only the size column needs a
      // client re-sort (folder sizes resolve asynchronously and can reorder
      // entries Rust couldn't know about yet).
      const firstChunk =
        pane.sortKey === 'size'
          ? sortEntries(head.firstChunk, pane.sortKey, pane.sortDirection, get().sizeStates)
          : head.firstChunk

      set((state) => {
        const previous = state.panes[paneId]
        const entries = withResolvedIcons(firstChunk, state.resolvedIconPaths)
        const restoredFocusId = findMatchingEntryId(firstChunk, [
          selection.focusedId,
          previous.focusedEntryId,
          ...selection.selectedIds,
        ])
        // Seed the back/forward stack the first time a pane resolves a real path
        // (initial load, or after a tab switch reset its history to empty).
        const seeded =
          previous.historyIndex === -1
            ? { history: [head.path], historyIndex: 0 }
            : { history: previous.history, historyIndex: previous.historyIndex }

        return {
          panes: {
            ...state.panes,
            [paneId]: {
              ...previous,
              path: head.path,
              entries,
              focusedEntryId: restoredFocusId ?? firstChunk[0]?.id ?? null,
              loading: false,
              listRequestId: head.requestId,
              ...seeded,
            },
          },
        }
      })

      const nextPane = get().panes[paneId]
      useTabsStore.getState().patchActiveTab(paneId, {
        path: nextPane.path,
        sortKey: nextPane.sortKey,
        sortDirection: nextPane.sortDirection,
        filter: nextPane.filterApplied,
      })
      schedulePersistSession(get().activePaneId)

      // When the listing already fits in the first chunk the tail runs now;
      // otherwise it runs from `applyListChunk` once the final chunk arrives.
      if (head.done) {
        await get().finalizeListing(paneId)
      }
    } catch (error) {
      log.error('reloadPane failed to load directory', {
        paneId,
        path: pane.path,
        error,
      })
      set((state) => ({
        panes: {
          ...state.panes,
          [paneId]: {
            ...state.panes[paneId],
            loading: false,
            error: error instanceof Error ? error.message : 'Failed to load directory',
          },
        },
      }))
    }
  },
  navigatePane: async (paneId, path, options = {}) => {
    log.debug('navigatePane', { paneId, path, viaHistory: options.viaHistory ?? false })
    const timer = get().filterTimers[paneId]
    if (timer && !options.viaHistory) {
      window.clearTimeout(timer)
    }

    // Abandon in-flight folder-size jobs for the folder we're leaving so they
    // stop churning once the pane moves on. Jobs for folders still shown by the
    // other pane (same directory open in both) are left untouched.
    const leaving = get().panes[paneId]
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
    })

    set((state) => ({
      activePaneId: paneId,
      focusRequestId: state.focusRequestId + 1,
      focusRequestPaneId: paneId,
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
          focusedEntryId: null,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

    await get().reloadPane(paneId)
  },
  closeTab: async (paneId, tabId) => {
    const before = useTabsStore.getState().panes[paneId]
    if (before.tabs.length <= 1) {
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
          focusedEntryId: null,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

    await get().reloadPane(paneId)
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
          focusedEntryId: null,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

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

    await get().reloadPane(paneId)
  },
  applyListChunk: (events) => {
    if (events.length === 0) {
      return
    }

    const completedPanes = new Set<PaneId>()
    set((state) => {
      const nextPanes = { ...state.panes }
      let changed = false

      for (const paneId of Object.keys(nextPanes) as PaneId[]) {
        const pane = nextPanes[paneId]
        const activeTabId = activeTab(paneId).id
        let entries: DirectoryEntry[] | null = null
        let completed = false

        for (const event of events) {
          // Only apply chunks for this pane's current tab and in-flight listing
          // request; anything from a superseded navigation is dropped.
          if (
            paneIdFromTabId(event.tabId) !== paneId ||
            event.tabId !== activeTabId ||
            event.requestId !== pane.listRequestId ||
            event.path !== pane.path
          ) {
            continue
          }

          // Rust streams globally-sorted chunks, so appending keeps order.
          if (!entries) {
            entries = [...pane.entries]
          }
          for (const entry of event.entries) {
            entries.push(hydrateEntryIcon(entry, state.resolvedIconPaths))
          }
          if (event.done) {
            completed = true
          }
        }

        if (!entries) {
          continue
        }

        // Folder sizes resolve asynchronously and are not continuously
        // re-sorted into the listing, so re-sort the whole accumulated list
        // once on completion for the size key (mirrors `patchEntries`).
        if (completed && pane.sortKey === 'size') {
          entries = sortEntries(entries, pane.sortKey, pane.sortDirection, state.sizeStates)
        }

        nextPanes[paneId] = { ...pane, entries }
        changed = true
        if (completed) {
          completedPanes.add(paneId)
        }
      }

      return changed ? { panes: nextPanes } : state
    })

    for (const paneId of completedPanes) {
      void get().finalizeListing(paneId)
    }
  },
  finalizeListing: async (paneId) => {
    const pane = get().panes[paneId]
    // Trash is a virtual location with no fs watcher / tree / size requests.
    if (isTrashPath(pane.path)) {
      return
    }

    const itemsColumnVisible =
      useLayoutStore.getState().columns.find((column) => column.key === 'items')?.visible ?? true

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
      await setTabWatch({
        tabId: activeTab(paneId).id,
        path: pane.path,
        sortKey: pane.sortKey,
        sortDirection: pane.sortDirection,
        filter: pane.filterApplied,
        showHidden: get().showHiddenFiles,
        includeItemCounts: itemsColumnVisible,
      })

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
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            entries,
            focusedEntryId: focusedStillPresent ? pane.focusedEntryId : (entries[0]?.id ?? null),
          },
        },
        ...(treeNodes ? { treeNodes } : {}),
      }
    }),
  setSort: async (paneId, sortKey) => {
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
      const sortedEntries = sortEntries(pane.entries, sortKey, sortDirection, state.sizeStates)

      return {
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            entries: sortedEntries,
            sortKey,
            sortDirection,
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

      return {
        pendingSizeRequests:
          terminalPaths.length > 0
            ? withPendingSizeRequests(state.pendingSizeRequests, terminalPaths, false)
            : state.pendingSizeRequests,
        sizeStates: nextSizeStates,
      }
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
      const paths: string[] = []
      for (const event of events) {
        paths.push(event.path)
        // Recorded even when `iconDataUrl` is null: a resolved-to-nothing
        // result must stop future requests just as much as a real icon does.
        resolvedIconPaths[event.path] = event.iconDataUrl
      }

      return {
        panes: panesChanged ? nextPanes : state.panes,
        pendingIconRequests: withPendingIconRequests(state.pendingIconRequests, paths, false),
        resolvedIconPaths,
      }
    }),
  setEverythingStatus: (everythingStatus) => set({ everythingStatus }),
  setVolumes: (volumes) =>
    set((state) => {
      const nextTree = buildRootTreeState(volumes, state.treeNodes)
      return {
        volumes,
        treeRoots: nextTree.treeRoots,
        treeNodes: nextTree.treeNodes,
      }
    }),
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

    try {
      const response = await listTreeChildren({
        path,
        showHidden: get().showHiddenFiles,
      })

      const directoryChildren = response.children
      set((state) => ({
        treeNodes: {
          ...state.treeNodes,
          [path]: {
            id: path,
            name: currentNode?.name ?? basenameFromPath(path),
            path,
            parentPath: currentNode?.parentPath ?? getParentPath(path),
            children: directoryChildren.map((entry) => entry.path),
            // Newly-seen volume roots should open on first hydration just like
            // the initial roots. Once a root has loaded, preserve the user's
            // explicit expanded/collapsed choice on later refreshes.
            expanded:
              currentNode == null
                ? true
                : currentNode.expanded || (currentNode.parentPath === null && !currentNode.loaded),
            loaded: true,
          },
          // Always refresh name/parentPath from this fresh listing (the source
          // of truth) rather than trusting an existing node, which may be a
          // placeholder created before its parent was ever listed (e.g. a pane
          // navigated straight to it) and so was named from its raw path.
          ...Object.fromEntries(
            directoryChildren.map((entry) => {
              const existing = state.treeNodes[entry.path]
              return [
                entry.path,
                existing
                  ? { ...existing, name: entry.name, parentPath: path }
                  : {
                      id: entry.path,
                      name: entry.name,
                      path: entry.path,
                      parentPath: path,
                      children: [],
                      expanded: false,
                      loaded: false,
                    },
              ]
            }),
          ),
        },
      }))
    } catch {
      set((state) => ({
        treeNodes: {
          ...state.treeNodes,
          [path]: {
            id: path,
            name: currentNode?.name ?? basenameFromPath(path),
            path,
            parentPath: currentNode?.parentPath ?? getParentPath(path),
            children: [],
            expanded:
              currentNode == null
                ? false
                : currentNode.expanded || (currentNode.parentPath === null && !currentNode.loaded),
            loaded: true,
          },
        },
      }))
    }
  },
  revealPath: async (path) => {
    if (!path) {
      return
    }

    // Only reveal paths that live under one of the tree roots (volumes); paths
    // outside any volume have no node to expand to.
    const root = get().treeRoots.find((candidate) => isPathInsideVolume(path, candidate))
    if (!root) {
      return
    }

    // Build the ancestor chain from the root down to the active path.
    const chain: string[] = []
    let current: string | null = path
    while (current) {
      chain.unshift(current)
      if (current.toLowerCase() === root.toLowerCase()) {
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
      const ancestors = new Set(chain.slice(0, -1).map((entry) => entry.toLowerCase()))
      const leaf = path.toLowerCase()
      const treeNodes = { ...state.treeNodes }
      for (const [nodePath, node] of Object.entries(state.treeNodes)) {
        const lowerPath = nodePath.toLowerCase()
        if (ancestors.has(lowerPath)) {
          if (!node.expanded) {
            treeNodes[nodePath] = { ...node, expanded: true }
          }
        } else if (lowerPath !== leaf && node.expanded) {
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
          }),
          expanded: !state.treeNodes[path]?.expanded,
        },
      },
    }))
  },
  reset: () => {
    useTabsStore.getState().reset()
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
