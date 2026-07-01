import { create } from 'zustand'
import {
  listTreeChildren,
  listDir,
  listTrash,
  refreshTab,
  requestFolderSize,
  requestFolderSizes,
  saveSession,
  setTabWatch,
} from '@/lib/ipc/commands'
import { isTrashPath, trashEntryToDirectoryEntry } from '@/lib/trash'
import type {
  DirectoryEntry,
  DirPatchEvent,
  EverythingStatus,
  SessionState,
  SizeStateEvent,
  SortDirection,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import { activeTab, fromSessionPane, toSessionPane, useTabsStore } from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
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
  visibleStartIndex: number
  visibleEndIndex: number
  scrollPositions: Record<string, number>
  history: string[]
  historyIndex: number
}

type PendingSizeRequests = Record<string, true>

type InitializePayload = {
  session: SessionState
  showHiddenFiles: boolean
  everythingStatus: EverythingStatus
  volumes: VolumeInfo[]
}

type PanesStore = {
  activePaneId: PaneId
  panes: Record<PaneId, PaneState>
  showHiddenFiles: boolean
  everythingStatus: EverythingStatus | null
  volumes: VolumeInfo[]
  sizeStates: Record<string, EntrySizeState>
  pendingSizeRequests: PendingSizeRequests
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
  applyDirPatch: (event: DirPatchEvent) => void
  setSort: (paneId: PaneId, sortKey: SortKey) => Promise<void>
  setFilterDraft: (paneId: PaneId, value: string) => void
  clearFilter: (paneId: PaneId) => void
  setFocusedEntry: (paneId: PaneId, entryId: string | null) => void
  setVisibleRange: (paneId: PaneId, startIndex: number, endIndex: number) => void
  setScrollPosition: (paneId: PaneId, path: string, scrollTop: number) => void
  applySizeState: (event: SizeStateEvent) => void
  setEverythingStatus: (status: EverythingStatus) => void
  setVolumes: (volumes: VolumeInfo[]) => void
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  requestManualSize: (paneId: PaneId, entryId: string) => Promise<void>
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
    visibleStartIndex: 0,
    visibleEndIndex: 40,
    scrollPositions: {},
    history: [],
    historyIndex: -1,
  }
}

function defaultState() {
  return {
    activePaneId: 'left' as PaneId,
    panes: {
      left: createPane('left', 'Left pane'),
      right: createPane('right', 'Right pane'),
    },
    showHiddenFiles: false,
    everythingStatus: null,
    volumes: [] as VolumeInfo[],
    sizeStates: {} as Record<string, EntrySizeState>,
    pendingSizeRequests: {} as PendingSizeRequests,
    treeNodes: {} as Record<string, TreeNodeState>,
    treeRoots: [] as string[],
    filterTimers: {} as Partial<Record<PaneId, number>>,
  }
}

function findMatchingEntryId(entries: DirectoryEntry[], candidates: Array<string | null | undefined>) {
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
  const direction = sortDirection === 'asc' ? 1 : -1

  return [...entries].sort((left, right) => {
    if (left.isDir !== right.isDir) {
      return left.isDir ? -1 : 1
    }

    if (sortKey === 'size') {
      const leftState = sizeStates[left.path]
      const rightState = sizeStates[right.path]
      const leftHasKnownSize = isKnownSize(left, leftState)
      const rightHasKnownSize = isKnownSize(right, rightState)

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
  })
}

function directoryPaths(entries: DirectoryEntry[]): string[] {
  return entries.filter((entry) => entry.isDir).map((entry) => entry.path)
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

// Eager folder-size requests are only valid where the Everything index can
// answer them cheaply for the whole dataset (Windows + Everything available).
// On macOS and on Windows without Everything, sizes are strictly manual
// (Space / "Calculate size") — see plan FR7 / M4. Network paths are handled by
// the Rust size layer (always N/A); we never pre-gate on those here.
function eagerSizesEnabled(everythingStatus: EverythingStatus | null): boolean {
  return everythingStatus?.isAvailable ?? false
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
 * reload. Returns the next entry list (re-sorted) or null when the patch does
 * not target the pane's current path.
 */
function patchEntries(
  pane: PaneState,
  event: DirPatchEvent,
  sizeStates: Record<string, EntrySizeState>,
  showHiddenFiles: boolean,
): DirectoryEntry[] | null {
  if (pane.path !== event.path) {
    return null
  }

  const byId = new Map(pane.entries.map((entry) => [entry.path, entry]))

  for (const removedPath of event.removed) {
    byId.delete(removedPath)
  }

  for (const change of event.changed) {
    if (change.entry && entryMatchesPane(change.entry, pane, showHiddenFiles)) {
      byId.set(change.entry.path, change.entry)
    } else {
      byId.delete(change.path)
    }
  }

  return sortEntries([...byId.values()], pane.sortKey, pane.sortDirection, sizeStates)
}

function entryMatchesPane(entry: DirectoryEntry, pane: PaneState, showHiddenFiles: boolean) {
  if (!showHiddenFiles && (entry.isHidden || entry.isSystem)) {
    return false
  }

  return (
    pane.filterApplied === '' || entry.name.toLowerCase().includes(pane.filterApplied.toLowerCase())
  )
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
    const requestDirectorySizes = async (paths: string[]) => {
      if (!eagerSizesEnabled(get().everythingStatus)) {
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
                entries: sortedEntries,
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

      const response = await listDir({
        path: pane.path,
        sortKey: pane.sortKey,
        sortDirection: pane.sortDirection,
        filter: pane.filterApplied,
        showHidden: get().showHiddenFiles,
      })

      const sortedEntries = sortEntries(
        response.entries,
        pane.sortKey,
        pane.sortDirection,
        get().sizeStates,
      )

      set((state) => {
        const previous = state.panes[paneId]
        const restoredFocusId = findMatchingEntryId(sortedEntries, [
          selection.focusedId,
          previous.focusedEntryId,
          ...selection.selectedIds,
        ])
        // Seed the back/forward stack the first time a pane resolves a real path
        // (initial load, or after a tab switch reset its history to empty).
        const seeded =
          previous.historyIndex === -1
            ? { history: [response.path], historyIndex: 0 }
            : { history: previous.history, historyIndex: previous.historyIndex }

        return {
          panes: {
            ...state.panes,
            [paneId]: {
              ...previous,
              path: response.path,
              entries: sortedEntries,
              focusedEntryId: restoredFocusId ?? sortedEntries[0]?.id ?? null,
              loading: false,
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

      await setTabWatch({
        tabId: activeTab(paneId).id,
        path: nextPane.path,
        sortKey: nextPane.sortKey,
        sortDirection: nextPane.sortDirection,
        filter: nextPane.filterApplied,
        showHidden: get().showHiddenFiles,
      })

      const requestedDirectories = await requestDirectorySizes(directoryPaths(nextPane.entries))

      log.debug('reloadPane done', {
        paneId,
        path: nextPane.path,
        entries: nextPane.entries.length,
        sizeRequests: requestedDirectories.length,
      })

      await get().ensureTreeChildren(getParentPath(nextPane.path) ?? nextPane.path)
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
            visibleStartIndex: 0,
            visibleEndIndex: 40,
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
          visibleStartIndex: 0,
          visibleEndIndex: 40,
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
          visibleStartIndex: 0,
          visibleEndIndex: 40,
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
          visibleStartIndex: 0,
          visibleEndIndex: 40,
          scrollPositions: {},
          history: [],
          historyIndex: -1,
        },
      },
    }))

    // Background-tab recheck (M3): refresh the freshly-activated tab and re-arm
    // its watcher, then apply the returned patch on top of the full reload.
    await get().reloadPane(paneId)

    try {
      const patch = await refreshTab({
        tabId: next.id,
        path: next.path,
        sortKey: next.sortKey,
        sortDirection: next.sortDirection,
        filter: next.filter,
        showHidden: get().showHiddenFiles,
      })
      get().applyDirPatch(patch)
    } catch {
      // A failed recheck leaves the full-reload result in place.
    }
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

      const nextEntries = patchEntries(pane, event, state.sizeStates, state.showHiddenFiles)
      if (!nextEntries) {
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
        entries: nextEntries.length,
        changed: event.changed.length,
        removed: event.removed.length,
      })

      const focusedStillPresent = nextEntries.some((entry) => entry.id === pane.focusedEntryId)

      return {
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            entries: nextEntries,
            focusedEntryId: focusedStillPresent
              ? pane.focusedEntryId
              : (nextEntries[0]?.id ?? null),
          },
        },
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
  setVisibleRange: (paneId, startIndex, endIndex) => {
    const previous = get().panes[paneId]
    if (previous.visibleStartIndex === startIndex && previous.visibleEndIndex === endIndex) {
      return
    }

    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          visibleStartIndex: startIndex,
          visibleEndIndex: endIndex,
        },
      },
    }))
  },
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
  applySizeState: (event) =>
    set((state) => {
      const nextSizeStates = {
        ...state.sizeStates,
        [event.path]: {
          state: event.state,
          sizeBytes: event.sizeBytes,
          source: event.source,
        },
      }

      return {
        pendingSizeRequests: isTerminalSizeState(event.state)
          ? withPendingSizeRequests(state.pendingSizeRequests, [event.path], false)
          : state.pendingSizeRequests,
        sizeStates: nextSizeStates,
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
            name: currentNode?.name ?? path,
            path,
            parentPath: currentNode?.parentPath ?? getParentPath(path),
            children: directoryChildren.map((entry) => entry.path),
            expanded: currentNode?.expanded ?? true,
            loaded: true,
          },
          ...Object.fromEntries(
            directoryChildren.map((entry) => [
              entry.path,
              state.treeNodes[entry.path] ?? {
                id: entry.path,
                name: entry.name,
                path: entry.path,
                parentPath: path,
                children: [],
                expanded: false,
                loaded: false,
              },
            ]),
          ),
        },
      }))
    } catch {
      set((state) => ({
        treeNodes: {
          ...state.treeNodes,
          [path]: {
            id: path,
            name: currentNode?.name ?? path,
            path,
            parentPath: currentNode?.parentPath ?? getParentPath(path),
            children: [],
            expanded: currentNode?.expanded ?? false,
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
            name: path,
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
  const { ensureTreeChildren, treeRoots } = usePanesStore.getState()
  await Promise.all(treeRoots.map((root) => ensureTreeChildren(root)))
}
