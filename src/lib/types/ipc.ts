export type ThemePreference = 'system' | 'light' | 'dark'
export type PaneMode = 'dual' | 'single'
export type TreeWidth = 'compact' | 'default' | 'wide'
export type Shortcut = string
export type CommandId =
  | 'open'
  | 'goUp'
  | 'refresh'
  | 'rename'
  | 'delete'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'copyToOtherPane'
  | 'moveToOtherPane'
  | 'newFolder'
  | 'newFile'
  | 'calculateSize'
  | 'openInNewTab'
  | 'openInOtherPane'
  | 'selectAll'
  | 'clearFilter'
  | 'showSettings'

export type ColumnKey = SortKey

export type PaneShell = {
  id: 'left' | 'right'
  title: string
  path: string
  placeholderHeading: string
  placeholderBody: string
}

export type TreeRoot = {
  id: string
  label: string
}

export type InitialShellResponse = {
  panes: PaneShell[]
  treeRoots: TreeRoot[]
}

export type DirectoryEntry = {
  id: string
  name: string
  path: string
  isDir: boolean
  sizeBytes: number | null
  itemCount: number | null
  typeLabel: string
  modifiedAt: string | null
  createdAt: string | null
  attributes: string[]
  isHidden: boolean
  isSystem: boolean
}

export type SortDirection = 'asc' | 'desc'
export type SortKey = 'name' | 'size' | 'items' | 'type' | 'modified' | 'created'

export type ListDirRequest = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
}

export type ListDirResponse = {
  path: string
  entries: DirectoryEntry[]
}

export type VolumeInfo = {
  mountRoot: string
  label: string
  totalBytes: number
  freeBytes: number
  isNetwork: boolean
}

export type ColumnConfig = {
  key: ColumnKey
  visible: boolean
}

export type LayoutConfig = {
  detailsVisible: boolean
  treeWidth: TreeWidth
  defaultPaneMode: PaneMode
  restoreSession: boolean
}

export type AppConfig = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  keybindings: Partial<Record<CommandId, Shortcut[]>>
  columns: ColumnConfig[]
  layout: LayoutConfig
}

export type SessionTab = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
}

export type SessionPane = {
  activeTabIndex: number
  tabs: SessionTab[]
}

export type SessionState = {
  activePane: 'left' | 'right'
  leftPath: string
  rightPath: string
  left?: SessionPane | null
  right?: SessionPane | null
}

export type SaveConfigRequest = {
  config: AppConfig
}

export type SaveSessionRequest = {
  session: SessionState
}

export type DirPatchEvent = {
  tabId: string
  path: string
  reason: 'refresh' | 'watch'
  changed: { path: string; entry: DirectoryEntry | null }[]
  removed: string[]
}

export type SizeStateEvent = {
  path: string
  state: 'unknown' | 'calculating' | 'ready' | 'error' | 'na'
  source: 'everything' | 'manual' | 'network'
  sizeBytes: number | null
}

export type EverythingStatus = {
  status: 'unsupported' | 'unavailable' | 'notReady' | 'available'
  isAvailable: boolean
}

export type FolderSizeRequest = {
  path: string
}

export type FolderSizesRequest = {
  paths: string[]
}

export type CancelSizeRequest = {
  path: string
}

export type CancelSizeResponse = {
  cancelled: boolean
}

export type CreateEntryRequest = {
  parent: string
  name: string
}

export type RenameEntryRequest = {
  path: string
  newName: string
}

export type DeleteEntriesRequest = {
  paths: string[]
}

export type OpenPathRequest = {
  path: string
}

export type WatchTarget = {
  tabId: string
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
}

export type SetTabWatchRequest = {
  target: WatchTarget | null
}

export type RefreshTabRequest = {
  target: WatchTarget
}

export type VolumesChangedEvent = {
  volumes: VolumeInfo[]
}

export type OpKind = 'copy' | 'move'

export type OpStatus =
  | 'pending'
  | 'active'
  | 'paused'
  | 'conflict'
  | 'completed'
  | 'failed'
  | 'cancelled'

export type ConflictResolution = 'replace' | 'skip' | 'rename'

export type OpItem = {
  sourcePath: string
  name: string
  sizeBytes: number
}

export type StartOpRequest = {
  kind: OpKind
  destinationDir: string
  items: OpItem[]
}

export type ConflictInfo = {
  operationId: string
  sourcePath: string
  destinationPath: string
  name: string
}

export type OpProgress = {
  operationId: string
  kind: OpKind
  status: OpStatus
  sourceDir: string
  destinationDir: string
  totalItems: number
  completedItems: number
  totalBytes: number
  copiedBytes: number
  progressPercent: number
  bytesPerSecond: number
  etaSeconds: number | null
  currentFileName: string | null
  currentFileCopiedBytes: number
  currentFileTotalBytes: number
  errorMessage: string | null
}

export type ThroughputSample = {
  percent: number
  rate: number
}

export type OpSnapshot = {
  progress: OpProgress
  conflict: ConflictInfo | null
}

export type QueueProgressEvent = OpProgress

export type QueueConflictEvent = ConflictInfo

/** Operation id of a transfer the backend has auto-removed from the queue. */
export type QueueRemovedEvent = string

export type OpIdRequest = {
  id: string
}

export type ReorderOpsRequest = {
  ids: string[]
}

export type ResolveConflictRequest = {
  id: string
  resolution: ConflictResolution
  applyToAll: boolean
  renameTo: string | null
}

export type WatchErrorEvent = {
  path: string
  message: string
}

export type IpcCommandMap = {
  get_initial_shell: {
    request: undefined
    response: InitialShellResponse
  }
  list_dir: {
    request: ListDirRequest
    response: ListDirResponse
  }
  create_folder: {
    request: CreateEntryRequest
    response: DirectoryEntry
  }
  create_file: {
    request: CreateEntryRequest
    response: DirectoryEntry
  }
  rename_entry: {
    request: RenameEntryRequest
    response: DirectoryEntry
  }
  delete_entries: {
    request: DeleteEntriesRequest
    response: void
  }
  open_path: {
    request: OpenPathRequest
    response: void
  }
  list_volumes: {
    request: undefined
    response: VolumeInfo[]
  }
  everything_status: {
    request: undefined
    response: EverythingStatus
  }
  request_folder_size: {
    request: FolderSizeRequest
    response: void
  }
  request_folder_sizes: {
    request: FolderSizesRequest
    response: void
  }
  cancel_size: {
    request: CancelSizeRequest
    response: CancelSizeResponse
  }
  set_tab_watch: {
    request: SetTabWatchRequest
    response: void
  }
  refresh_tab: {
    request: RefreshTabRequest
    response: DirPatchEvent
  }
  load_config: {
    request: undefined
    response: AppConfig
  }
  save_config: {
    request: SaveConfigRequest
    response: AppConfig
  }
  load_session: {
    request: undefined
    response: SessionState
  }
  save_session: {
    request: SaveSessionRequest
    response: SessionState
  }
  start_op: {
    request: StartOpRequest
    response: string
  }
  pause_op: {
    request: OpIdRequest
    response: void
  }
  resume_op: {
    request: OpIdRequest
    response: void
  }
  cancel_op: {
    request: OpIdRequest
    response: void
  }
  retry_op: {
    request: OpIdRequest
    response: void
  }
  reorder_ops: {
    request: ReorderOpsRequest
    response: void
  }
  resolve_conflict: {
    request: ResolveConflictRequest
    response: void
  }
  queue_snapshot: {
    request: undefined
    response: OpSnapshot[]
  }
  has_unfinished_ops: {
    request: undefined
    response: boolean
  }
}

export type IpcEventMap = {
  'dir://patch': DirPatchEvent
  'size://state': SizeStateEvent
  'volumes://changed': VolumesChangedEvent
  'queue://progress': QueueProgressEvent
  'queue://conflict': QueueConflictEvent
  'queue://removed': QueueRemovedEvent
  'watch://error': WatchErrorEvent
}
