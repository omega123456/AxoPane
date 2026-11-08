import type {
  AppConfig,
  LogEntry,
  LogLevel,
  SaveConfigRequest,
  SaveSessionRequest,
  SessionState,
} from './config'
import type {
  CreateEntryRequest,
  DeleteFromTrashRequest,
  EjectVolumeRequest,
  ListTrashResponse,
  MenuActionStatus,
  OpenPathRequest,
  ReadFileClipboardResponse,
  RenameEntryRequest,
  RestoreTrashRequest,
  StartNativeDragRequest,
  TrashEntriesRequest,
  WriteFileClipboardRequest,
} from './entries'
import type {
  DirectoryEntry,
  ListDirRequest,
  ListDirResponse,
  ListTreeChildrenRequest,
  ListTreeChildrenResponse,
  VolumeInfo,
} from './listing'
import type {
  GetDefaultApplicationRequest,
  GetDefaultApplicationResponse,
  InvokeNativeMenuRequest,
  ListApplicationsResponse,
  LoadNativeMenuRequest,
  LoadNativeMenuResponse,
  OpenWithRequest,
  SetDefaultApplicationRequest,
  ShowPropertiesRequest,
  WarmNativeMenusRequest,
} from './native-menu'
import type {
  LogFrontendRequest,
  OpIdRequest,
  OpSnapshot,
  ReorderOpsRequest,
  ResolveConflictRequest,
  SetWindowThemeRequest,
  StartOpRequest,
  WindowBackdrop,
} from './queue'
import type { SearchPage, SearchPageRequest, SearchQueryRequest } from './search'
import type {
  BeginNavigationRequest,
  BeginNavigationResponse,
  GetSessionRangeRequest,
  ReleaseSessionRequest,
  ReleaseSessionResponse,
  ReviseSessionViewRequest,
  SessionRangeResponse,
} from './session'
import type { InitialShellResponse } from './shell'
import type {
  CancelSizeRequest,
  CancelSizeResponse,
  CancelSizesRequest,
  CancelSizesResponse,
  CancelThumbnailsRequest,
  FolderSizeIndexStatus,
  FolderSizeRequest,
  FolderSizesRequest,
  RequestIconsRequest,
  RequestThumbnailsRequest,
  RequestThumbnailsResponse,
  SizeSubscriptionsRequest,
} from './sizes'
import type {
  ActiveItemsSortRequest,
  ActiveItemsSortResponse,
  SetTabWatchRequest,
  VisibleItemCountsRequest,
} from './watch'

export type IpcCommandMap = {
  get_initial_shell: {
    request: undefined
    response: InitialShellResponse
  }
  list_dir: {
    request: ListDirRequest
    response: ListDirResponse
  }
  begin_directory_session: {
    request: BeginNavigationRequest
    response: BeginNavigationResponse
  }
  get_directory_session_range: {
    request: GetSessionRangeRequest
    response: SessionRangeResponse
  }
  revise_directory_session_view: {
    request: ReviseSessionViewRequest
    response: BeginNavigationResponse
  }
  release_directory_session: {
    request: ReleaseSessionRequest
    response: ReleaseSessionResponse
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
  restart_as_admin: {
    request: undefined
    response: void
  }
  open_full_disk_access_settings: {
    request: undefined
    response: void
  }
  set_window_theme: {
    request: SetWindowThemeRequest
    response: void
  }
  get_window_backdrop: {
    request: undefined
    response: WindowBackdrop
  }
  write_file_clipboard: {
    request: WriteFileClipboardRequest
    response: void
  }
  read_file_clipboard: {
    request: undefined
    response: ReadFileClipboardResponse
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
  list_volumes: {
    request: undefined
    response: VolumeInfo[]
  }
  eject_volume: {
    request: EjectVolumeRequest
    response: MenuActionStatus
  }
  folder_size_index_status: {
    request: undefined
    response: FolderSizeIndexStatus
  }
  set_size_subscriptions: {
    request: SizeSubscriptionsRequest
    response: void
  }
  search_query: {
    request: SearchQueryRequest
    response: SearchPage
  }
  search_page: {
    request: SearchPageRequest
    response: SearchPage
  }
  search_close: {
    request: undefined
    response: void
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
  request_thumbnails: {
    request: RequestThumbnailsRequest
    response: RequestThumbnailsResponse
  }
  cancel_thumbnails: {
    request: CancelThumbnailsRequest
    response: void
  }
  request_visible_item_counts: {
    request: VisibleItemCountsRequest
    response: void
  }
  sort_active_items: {
    request: ActiveItemsSortRequest
    response: ActiveItemsSortResponse
  }
  cancel_size: {
    request: CancelSizeRequest
    response: CancelSizeResponse
  }
  cancel_sizes: {
    request: CancelSizesRequest
    response: CancelSizesResponse
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
  skip_op: {
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
  start_native_drag: {
    request: StartNativeDragRequest
    response: void
  }
}
