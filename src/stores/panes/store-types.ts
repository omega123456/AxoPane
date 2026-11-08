import type {
  DirPatchEvent,
  FolderSizeIndexStatus,
  IconStateEvent,
  ItemCountEvent,
  SessionPatchEvent,
  SessionState,
  SizeStateEvent,
  SortKey,
  VolumeInfo,
} from '@/lib/types/ipc'
import type { TabMoveResult } from '@/stores/tabs-store'
import type { PanesData } from '@/stores/panes/pane-state'
import type { PaneId } from '@/types/pane'

export type NavigateOptions = {
  viaHistory?: boolean
  revealPath?: string
}

export type InitializePayload = {
  session: SessionState
  showHiddenFiles: boolean
  folderSizeIndexStatus: FolderSizeIndexStatus
  volumes: VolumeInfo[]
}

export type PanesStore = PanesData & {
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
