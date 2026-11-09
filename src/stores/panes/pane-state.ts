import type {
  DirectoryEntry,
  FolderSizeIndexStatus,
  SizeStateEvent,
  SortDirection,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import type { LazyTreeNode } from '@/stores/panes/tree-cache'

export type SizeStateKind = SizeStateEvent['state']

export type TreeFoldReveals = {
  leaf: string | null
  revealedSiblings: string[]
  showHiddenLevels: boolean
}

export type TreeNodeState = Omit<LazyTreeNode, 'expandability' | 'lastAccess'> & {
  expandability?: LazyTreeNode['expandability']
  lastAccess?: number
}

export type TreeNodes = Record<string, TreeNodeState>

export type EntrySizeState = {
  state: SizeStateKind
  sizeBytes: number | null
  source: SizeStateEvent['source']
  naReason?: SizeStateEvent['naReason']
  isLowerBound?: boolean
  skippedRoots?: string[]
  skippedMore?: number
  refreshing?: boolean
}

export type SizeStates = Record<string, EntrySizeState>

export type ItemsSortStatus = 'idle' | 'counting' | 'complete' | 'stale' | 'failed'

export type PaneState = {
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
  itemsSortStatus: ItemsSortStatus
  itemsSortOperationId: number
  error: string | null
  listRequestId: number
  listingGeneration: number
  acceptedChunkIndexes: number[]
  listingComplete: boolean
  scrollPositions: Record<string, number>
  history: string[]
  historyIndex: number
}

export type PaneRecord = Record<PaneId, PaneState>

export type PendingRequests = Record<string, true>

export type ResolvedIconPaths = Record<string, string | null>

export type PanesData = {
  activePaneId: PaneId
  focusRequestId: number
  focusRequestPaneId: PaneId | null
  panes: PaneRecord
  showHiddenFiles: boolean
  folderSizeIndexStatus: FolderSizeIndexStatus | null
  volumes: VolumeInfo[]
  sizeStates: SizeStates
  pendingSizeRequests: PendingRequests
  pendingIconRequests: PendingRequests
  passiveItemCountRequests: PendingRequests
  resolvedIconPaths: ResolvedIconPaths
  iconCacheTouched: Record<string, number>
  treeNodes: TreeNodes
  treeRoots: string[]
  treeFold: TreeFoldReveals
  filterTimers: Partial<Record<PaneId, number>>
  revealScrollRequests: Partial<Record<PaneId, number>>
}

export type PanesPatch = Pick<PanesData, 'panes'>

export const CLOSED_TREE_FOLD: TreeFoldReveals = {
  leaf: null,
  revealedSiblings: [],
  showHiddenLevels: false,
}

export function createPane(id: PaneId, title: string, path = '.'): PaneState {
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

export function defaultPanesData(): PanesData {
  return {
    activePaneId: 'left',
    focusRequestId: 0,
    focusRequestPaneId: null,
    panes: {
      left: createPane('left', 'Left pane'),
      right: createPane('right', 'Right pane'),
    },
    showHiddenFiles: false,
    folderSizeIndexStatus: null,
    volumes: [],
    sizeStates: {},
    pendingSizeRequests: {},
    pendingIconRequests: {},
    passiveItemCountRequests: {},
    resolvedIconPaths: {},
    iconCacheTouched: {},
    treeNodes: {},
    treeRoots: [],
    treeFold: CLOSED_TREE_FOLD,
    filterTimers: {},
    revealScrollRequests: {},
  }
}

export function withPane(state: PanesPatch, paneId: PaneId, patch: Partial<PaneState>): PanesPatch {
  return { panes: { ...state.panes, [paneId]: { ...state.panes[paneId], ...patch } } }
}

export function isListingSettled(pane: PaneState) {
  return !pane.loading && pane.listingComplete
}

function hasEntryWithId(entries: DirectoryEntry[], entryId: string) {
  return entries.some((entry) => entry.id === entryId)
}

export function findMatchingEntryId(
  entries: DirectoryEntry[],
  candidates: Array<string | null | undefined>,
) {
  for (const candidate of candidates) {
    if (!candidate) {
      continue
    }
    if (hasEntryWithId(entries, candidate)) {
      return candidate
    }
  }

  return null
}

export function retainedFocusId(entries: DirectoryEntry[], focusedEntryId: string | null) {
  if (entries.some((entry) => entry.id === focusedEntryId)) {
    return focusedEntryId
  }
  return entries[0]?.id ?? null
}

export type EntryUpdater<Update> = {
  locate: (update: Update) => number
  apply: (entry: DirectoryEntry, update: Update) => DirectoryEntry | null
}

export function updateEntries<Update>(
  entries: DirectoryEntry[],
  updates: Update[],
  { locate, apply }: EntryUpdater<Update>,
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

export function patchPaneEntries(
  panes: PaneRecord,
  patchPane: (paneId: PaneId, pane: PaneState) => DirectoryEntry[] | null,
): PaneRecord | null {
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

export function directoryPaths(entries: DirectoryEntry[]): string[] {
  return entries.filter((entry) => entry.isDir).map((entry) => entry.path)
}
