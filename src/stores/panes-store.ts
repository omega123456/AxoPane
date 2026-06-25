import { create } from 'zustand'
import {
  listDir,
  refreshTab,
  requestFolderSize,
  requestFolderSizes,
  saveSession,
  setTabWatch,
} from '@/lib/ipc/commands'
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
import {
  activeTab,
  fromSessionPane,
  toSessionPane,
  useTabsStore,
} from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
import { log } from '@/lib/app-log-commands'

type PaneId = 'left' | 'right'
type SizeStateKind = SizeStateEvent['state']

type TreeNodeState = {
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
}

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
  treeNodes: Record<string, TreeNodeState>
  treeRoots: string[]
  filterTimers: Partial<Record<PaneId, number>>
  initialize: (payload: InitializePayload) => void
  setActivePane: (paneId: PaneId) => void
  reloadPane: (paneId: PaneId) => Promise<void>
  navigatePane: (paneId: PaneId, path: string) => Promise<void>
  goUp: (paneId: PaneId) => Promise<void>
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
  applySizeState: (event: SizeStateEvent) => void
  setEverythingStatus: (status: EverythingStatus) => void
  setVolumes: (volumes: VolumeInfo[]) => void
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  requestManualSize: (paneId: PaneId, entryId: string) => Promise<void>
  ensureTreeChildren: (path: string | null) => Promise<void>
  toggleTreeNode: (path: string) => Promise<void>
  reset: () => void
}

const filterDelayMs = 180

function createPane(id: PaneId, title: string, path = '.') : PaneState {
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
    treeNodes: {} as Record<string, TreeNodeState>,
    treeRoots: [] as string[],
    filterTimers: {} as Partial<Record<PaneId, number>>,
  }
}

export function getParentPath(path: string): string | null {
  if (!path) {
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

function entrySortValue(entry: DirectoryEntry, sizeState: EntrySizeState | undefined, sortKey: SortKey) {
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

    const leftValue = entrySortValue(left, sizeStates[left.path], sortKey)
    const rightValue = entrySortValue(right, sizeStates[right.path], sortKey)

    if (leftValue < rightValue) {
      return -1 * direction
    }

    if (leftValue > rightValue) {
      return 1 * direction
    }

    return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: 'base' })
  })
}

/**
 * Collects the directory entry paths inside the pane's current visible window.
 * Folder sizes are requested for exactly these paths so the Rust size layer can
 * pick the right backend (Everything / manual / network-N/A) per path.
 */
function visibleDirectoryPaths(pane: PaneState): string[] {
  return pane.entries
    .slice(pane.visibleStartIndex, pane.visibleEndIndex + 1)
    .filter((entry) => entry.isDir)
    .map((entry) => entry.path)
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
): DirectoryEntry[] | null {
  if (pane.path !== event.path) {
    return null
  }

  const byId = new Map(pane.entries.map((entry) => [entry.path, entry]))

  for (const removedPath of event.removed) {
    byId.delete(removedPath)
  }

  for (const change of event.changed) {
    if (change.entry) {
      byId.set(change.entry.path, change.entry)
    } else {
      byId.delete(change.path)
    }
  }

  return sortEntries([...byId.values()], pane.sortKey, pane.sortDirection, sizeStates)
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
        },
        right: {
          ...state.panes.right,
          path: activeRight.path,
          sortKey: activeRight.sortKey,
          sortDirection: activeRight.sortDirection,
          filterDraft: activeRight.filter,
          filterApplied: activeRight.filter,
          title: 'Right pane',
        },
      },
      treeRoots: volumes.map((volume) => volume.mountRoot),
      treeNodes: Object.fromEntries(
        volumes.map((volume) => [
          volume.mountRoot,
          {
            id: volume.mountRoot,
            name: volume.label || volume.mountRoot,
            path: volume.mountRoot,
            parentPath: null,
            children: [],
            expanded: false,
            loaded: false,
          },
        ]),
      ),
    }))
  },
  setActivePane: (paneId) => {
    set({ activePaneId: paneId })
    schedulePersistSession(paneId)
  },
  reloadPane: async (paneId) => {
    const pane = get().panes[paneId]
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: { ...state.panes[paneId], loading: true, error: null },
      },
    }))

    try {
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

      set((state) => ({
        panes: {
          ...state.panes,
          [paneId]: {
            ...state.panes[paneId],
            path: response.path,
            entries: sortedEntries,
            focusedEntryId: sortedEntries[0]?.id ?? null,
            loading: false,
          },
        },
      }))

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

      // Eager-request folder sizes for the visible directory entries only when
      // the Everything index is available (Windows). On macOS / no-Everything
      // Windows, sizes stay manual (Space / "Calculate size") per FR7 / M4.
      const visibleDirectories = eagerSizesEnabled(get().everythingStatus)
        ? visibleDirectoryPaths(nextPane)
        : []
      if (visibleDirectories.length > 0) {
        await requestFolderSizes({ paths: visibleDirectories })
      }

      log.debug('reloadPane done', {
        paneId,
        path: nextPane.path,
        entries: nextPane.entries.length,
        sizeRequests: visibleDirectories.length,
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
  navigatePane: async (paneId, path) => {
    log.debug('navigatePane', { paneId, path })
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          ...state.panes[paneId],
          path,
        },
      },
    }))
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
      for (const entry of pane.entries) {
        delete nextSizeStates[entry.path]
      }
      return { sizeStates: nextSizeStates }
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

      const nextEntries = patchEntries(pane, event, state.sizeStates)
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
            focusedEntryId: focusedStillPresent ? pane.focusedEntryId : (nextEntries[0]?.id ?? null),
          },
        },
      }
    }),
  setSort: async (paneId, sortKey) => {
    set((state) => {
      const pane = state.panes[paneId]
      const sortDirection =
        pane.sortKey === sortKey ? (pane.sortDirection === 'asc' ? 'desc' : 'asc') : sortKey === 'name' || sortKey === 'type' ? 'asc' : 'desc'

      return {
        panes: {
          ...state.panes,
          [paneId]: {
            ...pane,
            sortKey,
            sortDirection,
          },
        },
      }
    })

    await get().reloadPane(paneId)
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

    // Newly scrolled-into-view folders need their sizes requested too; the
    // initial reload only covered the first visible window. Request sizes for
    // folders that don't yet have a size state recorded — but only where eager
    // sizing applies (Windows + Everything); otherwise sizing stays manual.
    if (!eagerSizesEnabled(get().everythingStatus)) {
      return
    }

    const pane = get().panes[paneId]
    const pending = visibleDirectoryPaths(pane).filter((path) => !get().sizeStates[path])
    if (pending.length > 0) {
      void requestFolderSizes({ paths: pending }).catch((error) => {
        log.warn('setVisibleRange size request failed', { paneId, error })
      })
    }
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

      const nextPanes = Object.fromEntries(
        (Object.entries(state.panes) as [PaneId, PaneState][]).map(([paneId, pane]) => [
          paneId,
          pane.sortKey === 'size'
            ? {
                ...pane,
                entries: sortEntries(pane.entries, pane.sortKey, pane.sortDirection, nextSizeStates),
              }
            : pane,
        ]),
      ) as Record<PaneId, PaneState>

      return {
        sizeStates: nextSizeStates,
        panes: nextPanes,
      }
    }),
  setEverythingStatus: (everythingStatus) => set({ everythingStatus }),
  setVolumes: (volumes) => set({ volumes }),
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
      const response = await listDir({
        path,
        sortKey: 'name',
        sortDirection: 'asc',
        filter: '',
        showHidden: get().showHiddenFiles,
      })

      const directoryChildren = response.entries.filter((entry) => entry.isDir)
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
