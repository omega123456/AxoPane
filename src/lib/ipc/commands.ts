import type {
  AppConfig,
  ActiveItemsSortRequest,
  ActiveItemsSortResponse,
  BeginNavigationRequest,
  BeginNavigationResponse,
  CancelSizeResponse,
  CancelSizesResponse,
  CreateEntryRequest,
  DeleteFromTrashRequest,
  DirectoryEntry,
  EjectVolumeRequest,
  EverythingStatus,
  FolderSizeRequest,
  FolderSizesRequest,
  GetDefaultApplicationRequest,
  GetDefaultApplicationResponse,
  GetSessionRangeRequest,
  InitialShellResponse,
  InvokeNativeMenuRequest,
  ListApplicationsResponse,
  ListDirRequest,
  ListDirResponse,
  ListTreeChildrenRequest,
  ListTreeChildrenResponse,
  ListTrashResponse,
  LoadNativeMenuRequest,
  LoadNativeMenuResponse,
  LogEntry,
  LogLevel,
  MenuActionStatus,
  OpenWithRequest,
  OpenPathRequest,
  ReleaseSessionRequest,
  ReleaseSessionResponse,
  RenameEntryRequest,
  RequestIconsRequest,
  RestoreTrashRequest,
  ReviseSessionViewRequest,
  SessionRangeResponse,
  SetDefaultApplicationRequest,
  ShowPropertiesRequest,
  TrashEntriesRequest,
  VisibleItemCountsRequest,
  WarmNativeMenusRequest,
  WatchTarget,
  SessionState,
  VolumeInfo,
  WatchSeedReference,
  WriteFileClipboardRequest,
} from '@/lib/types/ipc'
import { invokeCommand } from './client'

export function getInitialShell() {
  return invokeCommand({ command: 'get_initial_shell' }) as Promise<InitialShellResponse>
}

export function listDir(payload: ListDirRequest) {
  return invokeCommand({ command: 'list_dir', payload }) as Promise<ListDirResponse>
}

/**
 * v2 seekable directory-session commands (Phase 3 backend, Phase 4
 * frontend). `getDirectorySessionRange` and `reviseDirectorySessionView`
 * reject with the typed `SessionRejection` shape (see `lib/types/ipc.ts`),
 * not a plain string — callers should branch on `error.kind` rather than
 * string-matching.
 */
export function beginDirectorySession(payload: BeginNavigationRequest) {
  return invokeCommand({
    command: 'begin_directory_session',
    payload,
  }) as Promise<BeginNavigationResponse>
}

export function getDirectorySessionRange(payload: GetSessionRangeRequest) {
  return invokeCommand({
    command: 'get_directory_session_range',
    payload,
  }) as Promise<SessionRangeResponse>
}

export function reviseDirectorySessionView(payload: ReviseSessionViewRequest) {
  return invokeCommand({
    command: 'revise_directory_session_view',
    payload,
  }) as Promise<BeginNavigationResponse>
}

export function releaseDirectorySession(payload: ReleaseSessionRequest) {
  return invokeCommand({
    command: 'release_directory_session',
    payload,
  }) as Promise<ReleaseSessionResponse>
}

export function listTreeChildren(payload: ListTreeChildrenRequest) {
  return invokeCommand({
    command: 'list_tree_children',
    payload,
  }) as Promise<ListTreeChildrenResponse>
}

export function createFolder(payload: CreateEntryRequest) {
  return invokeCommand({ command: 'create_folder', payload }) as Promise<DirectoryEntry>
}

export function createFile(payload: CreateEntryRequest) {
  return invokeCommand({ command: 'create_file', payload }) as Promise<DirectoryEntry>
}

export function renameEntry(payload: RenameEntryRequest) {
  return invokeCommand({ command: 'rename_entry', payload }) as Promise<DirectoryEntry>
}

export function moveToTrash(payload: TrashEntriesRequest) {
  return invokeCommand({ command: 'move_to_trash', payload }) as Promise<void>
}

export function listTrash() {
  return invokeCommand({ command: 'list_trash' }) as Promise<ListTrashResponse>
}

export function restoreFromTrash(payload: RestoreTrashRequest) {
  return invokeCommand({ command: 'restore_from_trash', payload }) as Promise<void>
}

export function emptyTrash() {
  return invokeCommand({ command: 'empty_trash' }) as Promise<void>
}

export function deleteFromTrash(payload: DeleteFromTrashRequest) {
  return invokeCommand({ command: 'delete_from_trash', payload }) as Promise<void>
}

export function openPath(payload: OpenPathRequest) {
  return invokeCommand({ command: 'open_path', payload }) as Promise<void>
}

export function writeFileClipboard(payload: WriteFileClipboardRequest) {
  return invokeCommand({ command: 'write_file_clipboard', payload }) as Promise<void>
}

export function clearFileClipboard() {
  return invokeCommand({ command: 'clear_file_clipboard' }) as Promise<void>
}

export function loadNativeMenu(payload: LoadNativeMenuRequest) {
  return invokeCommand({ command: 'load_native_menu', payload }) as Promise<LoadNativeMenuResponse>
}

export function warmNativeMenus(payload: WarmNativeMenusRequest) {
  return invokeCommand({ command: 'warm_native_menus', payload }) as Promise<void>
}

export function invokeNativeMenuAction(payload: InvokeNativeMenuRequest) {
  return invokeCommand({
    command: 'invoke_native_menu_action',
    payload,
  }) as Promise<MenuActionStatus>
}

export function showProperties(payload: ShowPropertiesRequest) {
  return invokeCommand({ command: 'show_properties', payload }) as Promise<MenuActionStatus>
}

export function openWith(payload: OpenWithRequest) {
  return invokeCommand({ command: 'open_with', payload }) as Promise<MenuActionStatus>
}

export function listApplications() {
  return invokeCommand({ command: 'list_applications' }) as Promise<ListApplicationsResponse>
}

export function setDefaultApplication(payload: SetDefaultApplicationRequest) {
  return invokeCommand({ command: 'set_default_application', payload }) as Promise<MenuActionStatus>
}

export function getDefaultApplication(payload: GetDefaultApplicationRequest) {
  return invokeCommand({
    command: 'get_default_application',
    payload,
  }) as Promise<GetDefaultApplicationResponse>
}

export function listVolumes() {
  return invokeCommand({ command: 'list_volumes' }) as Promise<VolumeInfo[]>
}

export function ejectVolume(payload: EjectVolumeRequest) {
  return invokeCommand({ command: 'eject_volume', payload }) as Promise<MenuActionStatus>
}

export function everythingStatus() {
  return invokeCommand({ command: 'everything_status' }) as Promise<EverythingStatus>
}

export function requestFolderSize(payload: FolderSizeRequest) {
  return invokeCommand({ command: 'request_folder_size', payload }) as Promise<void>
}

export function requestFolderSizes(payload: FolderSizesRequest) {
  return invokeCommand({ command: 'request_folder_sizes', payload }) as Promise<void>
}

export function requestIcons(payload: RequestIconsRequest) {
  return invokeCommand({ command: 'request_icons', payload }) as Promise<void>
}

export function requestVisibleItemCounts(payload: VisibleItemCountsRequest) {
  return invokeCommand({ command: 'request_visible_item_counts', payload }) as Promise<void>
}

export function sortActiveItems(payload: ActiveItemsSortRequest) {
  return invokeCommand({ command: 'sort_active_items', payload }) as Promise<ActiveItemsSortResponse>
}

export function cancelSize(path: string) {
  return invokeCommand({ command: 'cancel_size', payload: { path } }) as Promise<CancelSizeResponse>
}

export function cancelSizes(paths: string[]) {
  return invokeCommand({
    command: 'cancel_sizes',
    payload: { paths },
  }) as Promise<CancelSizesResponse>
}

export function setTabWatch(
  target: WatchTarget | null,
  entries?: DirectoryEntry[],
  seedReference?: WatchSeedReference,
) {
  return invokeCommand({
    command: 'set_tab_watch',
    payload: { target, seedReference, entries },
  }) as Promise<void>
}

export function loadConfig() {
  return invokeCommand({ command: 'load_config' }) as Promise<AppConfig>
}

export function saveConfig(config: AppConfig) {
  return invokeCommand({ command: 'save_config', payload: { config } }) as Promise<AppConfig>
}

export function loadSession() {
  return invokeCommand({ command: 'load_session' }) as Promise<SessionState>
}

export function saveSession(session: SessionState) {
  return invokeCommand({ command: 'save_session', payload: { session } }) as Promise<SessionState>
}

export function readLogs() {
  return invokeCommand({ command: 'read_logs' }) as Promise<LogEntry[]>
}

export function setLogLevel(level: LogLevel) {
  return invokeCommand({ command: 'set_log_level', payload: { level } }) as Promise<void>
}
