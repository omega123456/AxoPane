import type { DateFormat } from '@/lib/date-format'
import type { UpdateInterval } from '@/lib/update-intervals'

export type ThemePreference = 'system' | 'light' | 'dark'
export type PaneMode = 'dual' | 'single'
export type ZoomLevel = '80' | '90' | '100' | '110' | '120' | '125' | '150'
export type Shortcut = string
export type CommandId =
  | 'open'
  | 'goUp'
  | 'refresh'
  | 'rename'
  | 'delete'
  | 'deletePermanent'
  | 'restore'
  | 'emptyTrash'
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
  iconDataUrl?: string | null
  sizeBytes: number | null
  itemCount: number | null
  typeLabel: string
  modifiedAt: string | null
  createdAt: string | null
  attributes: string[]
  isHidden: boolean
  isSystem: boolean
  /** Set only for rows synthesized from a trash listing: the opaque id used to restore/purge this item. */
  trashId?: string
  /** Set only for rows synthesized from a trash listing, when the original location is known. */
  originalPath?: string | null
}

export type SortDirection = 'asc' | 'desc'
export type SortKey = 'name' | 'size' | 'items' | 'type' | 'modified' | 'created'

export type ListDirRequest = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
  includeItemCounts: boolean
}

export type ListDirResponse = {
  path: string
  entries: DirectoryEntry[]
}

export type TreeChildEntry = {
  name: string
  path: string
  hasChildren: boolean
}

export type ListTreeChildrenRequest = {
  path: string
  showHidden: boolean
}

export type ListTreeChildrenResponse = {
  path: string
  children: TreeChildEntry[]
}

export type VolumeInfo = {
  mountRoot: string
  label: string
  totalBytes: number
  freeBytes: number
  isNetwork: boolean
  isRemovable: boolean
}

export type VolumeCategory = 'fixed' | 'removable' | 'network'

export type VolumeGroup = {
  category: VolumeCategory
  label: string
  volumes: VolumeInfo[]
}

export type ColumnConfig = {
  key: ColumnKey
  visible: boolean
}

export type ColumnWidths = Partial<Record<ColumnKey, number>>

export type LayoutConfig = {
  detailsVisible: boolean
  /** Folder-tree sidebar width in pixels (user-draggable). */
  treeWidthPx: number
  /** Fraction of the dual-pane area allotted to the left pane (0..1, user-draggable). */
  paneSplit: number
  /** Per-column widths in CSS pixels. Missing keys fall back to app defaults. */
  columnWidths: ColumnWidths
  defaultPaneMode: PaneMode
  restoreSession: boolean
  zoom: ZoomLevel
}

export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

export type LogDisplayFilter = 'all' | LogLevel

export type LogEntry = {
  id: number
  timestamp: string
  level: string
  target: string
  message: string
}

export type AppConfig = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  keybindings: Partial<Record<CommandId, Shortcut[]>>
  columns: ColumnConfig[]
  layout: LayoutConfig
  updateCheckInterval: UpdateInterval
  logLevel: LogLevel
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
  relativeDates: boolean
  autoFolderSize: boolean
  autoExpandActiveQueueToasts: boolean
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

export type RequestIconsRequest = {
  paths: string[]
}

export type IconStateEvent = {
  path: string
  iconDataUrl: string | null
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

export type TrashEntriesRequest = {
  paths: string[]
}

export type TrashEntry = {
  id: string
  name: string
  originalPath: string | null
  sizeBytes: number | null
  isDir: boolean
  deletedAt: number | null
}

export type ListTrashResponse = {
  entries: TrashEntry[]
}

export type RestoreTrashRequest = {
  ids: string[]
}

export type DeleteFromTrashRequest = {
  ids: string[]
}

export type OpenPathRequest = {
  path: string
}

export type FileClipboardMode = 'copy' | 'move'

export type WriteFileClipboardRequest = {
  mode: FileClipboardMode
  paths: string[]
}

export type MenuActionStatus = {
  handled: boolean
  message?: string | null
}

export type NativeMenuTargetKind =
  | 'file'
  | 'folder'
  | 'multi'
  | 'mixed'
  | 'driveRoot'
  | 'background'
  | 'tree'
  | 'tab'

export type NativeMenuCanonicalActionKind =
  | 'open'
  | 'openWith'
  | 'copy'
  | 'copyAsPath'
  | 'cut'
  | 'paste'
  | 'rename'
  | 'delete'
  | 'properties'
  | 'share'
  | 'compress'
  | 'extract'
  | 'refresh'
  | 'newFolder'
  | 'newFile'
  | 'selectAll'

export type NativeMenuIconKind = 'dataUrl'

export type NativeMenuIcon = {
  kind: NativeMenuIconKind
  dataUrl: string
  alt?: string | null
}

export type NativeMenuItem = {
  id: string
  label: string
  enabled: boolean
  danger: boolean
  canonicalActionKind: NativeMenuCanonicalActionKind | null
  normalizedVerb: string | null
  invokeToken: string | null
  icon: NativeMenuIcon | null
  children: NativeMenuItem[]
}

export type LoadNativeMenuRequest = {
  requestId: string
  targetKind: NativeMenuTargetKind
  targetPath: string | null
  folderPath: string | null
  selectedPaths: string[]
}

export type LoadNativeMenuResponse = {
  requestId: string
  items: NativeMenuItem[]
}

/**
 * Batch of representative single-item native menu requests used to
 * proactively warm the backend's native-menu cache in the background. Each
 * element is a full `LoadNativeMenuRequest` (not the context-menu module's
 * `Omit<LoadNativeMenuRequest, 'requestId'>` request type) so it can carry a
 * throwaway `requestId`.
 */
export type WarmNativeMenusRequest = {
  requests: LoadNativeMenuRequest[]
}

export type InvokeNativeMenuRequest = {
  token: string
}

export type ShowPropertiesRequest = {
  paths: string[]
}

export type OpenWithRequest = {
  path: string
}

export type MacApp = {
  name: string
  bundlePath: string
  bundleId: string | null
  iconDataUrl: string | null
}

export type ListApplicationsResponse = {
  apps: MacApp[]
}

export type SetDefaultApplicationRequest = {
  path: string
  bundlePath: string
}

export type GetDefaultApplicationRequest = {
  path: string
}

export type GetDefaultApplicationResponse = {
  app: MacApp | null
}

export type CompressArchiveRequest = {
  paths: string[]
  destinationDir: string
}

export type ExtractArchiveRequest = {
  paths: string[]
  destinationDir: string
}

export type WatchTarget = {
  tabId: string
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
  includeItemCounts: boolean
}

export type SetTabWatchRequest = {
  target: WatchTarget | null
  /** Post-sort/filter entries already fetched for this listing, used to seed the watcher baseline. */
  entries?: DirectoryEntry[]
}

export type VolumesChangedEvent = {
  volumes: VolumeInfo[]
}

export type OpKind = 'copy' | 'move' | 'delete' | 'compress' | 'extract'

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
  itemNames: string[]
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

export type FrontendLogLevel = 'debug' | 'info' | 'warn' | 'error'

export type LogFrontendRequest = {
  level: FrontendLogLevel
  message: string
  category?: string
  details?: string
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
  list_tree_children: {
    request: ListTreeChildrenRequest
    response: ListTreeChildrenResponse
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
  move_to_trash: {
    request: TrashEntriesRequest
    response: void
  }
  list_trash: {
    request: undefined
    response: ListTrashResponse
  }
  restore_from_trash: {
    request: RestoreTrashRequest
    response: void
  }
  empty_trash: {
    request: undefined
    response: void
  }
  delete_from_trash: {
    request: DeleteFromTrashRequest
    response: void
  }
  open_path: {
    request: OpenPathRequest
    response: void
  }
  write_file_clipboard: {
    request: WriteFileClipboardRequest
    response: void
  }
  clear_file_clipboard: {
    request: undefined
    response: void
  }
  load_native_menu: {
    request: LoadNativeMenuRequest
    response: LoadNativeMenuResponse
  }
  warm_native_menus: {
    request: WarmNativeMenusRequest
    response: void
  }
  invoke_native_menu_action: {
    request: InvokeNativeMenuRequest
    response: MenuActionStatus
  }
  show_properties: {
    request: ShowPropertiesRequest
    response: MenuActionStatus
  }
  open_with: {
    request: OpenWithRequest
    response: MenuActionStatus
  }
  list_applications: {
    request: undefined
    response: ListApplicationsResponse
  }
  set_default_application: {
    request: SetDefaultApplicationRequest
    response: MenuActionStatus
  }
  get_default_application: {
    request: GetDefaultApplicationRequest
    response: GetDefaultApplicationResponse
  }
  compress_archive: {
    request: CompressArchiveRequest
    response: MenuActionStatus
  }
  extract_archive: {
    request: ExtractArchiveRequest
    response: MenuActionStatus
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
  request_icons: {
    request: RequestIconsRequest
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
  log_frontend: {
    request: LogFrontendRequest
    response: void
  }
  read_logs: {
    request: undefined
    response: LogEntry[]
  }
  set_log_level: {
    request: { level: LogLevel }
    response: void
  }
}

export type IpcEventMap = {
  'dir://patch': DirPatchEvent
  'size://state': SizeStateEvent
  /** Batched: the backend flushes resolved icons in chunks (see Phase 3 backend batching). */
  'icon://state': IconStateEvent[]
  'volumes://changed': VolumesChangedEvent
  'queue://progress': QueueProgressEvent
  'queue://conflict': QueueConflictEvent
  'queue://removed': QueueRemovedEvent
  'watch://error': WatchErrorEvent
}
