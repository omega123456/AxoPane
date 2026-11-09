import type {
  AppConfig,
  ActiveItemsSortRequest,
  BeginNavigationRequest,
  CreateEntryRequest,
  DeleteFromTrashRequest,
  DirectoryEntry,
  EjectVolumeRequest,
  FolderSizeRequest,
  FolderSizesRequest,
  GetDefaultApplicationRequest,
  GetSessionRangeRequest,
  InvokeNativeMenuRequest,
  ListDirRequest,
  ListTreeChildrenRequest,
  LoadNativeMenuRequest,
  LogLevel,
  OpenWithRequest,
  OpenPathRequest,
  ReleaseSessionRequest,
  RenameEntryRequest,
  RequestIconsRequest,
  RequestThumbnailsRequest,
  CancelThumbnailsRequest,
  RestoreTrashRequest,
  ReviseSessionViewRequest,
  SearchPageRequest,
  SearchQueryRequest,
  SetDefaultApplicationRequest,
  SizeSubscriptionsRequest,
  ShowPropertiesRequest,
  StartNativeDragRequest,
  TrashEntriesRequest,
  VisibleItemCountsRequest,
  WarmNativeMenusRequest,
  WatchTarget,
  SessionState,
  WatchSeedReference,
  SetWindowThemeRequest,
  WriteFileClipboardRequest,
} from '@/lib/types/ipc'
import { invokeCommand } from './client'

export function getInitialShell() {
  return invokeCommand({ command: 'get_initial_shell' })
}

export function listDir(payload: ListDirRequest) {
  return invokeCommand({ command: 'list_dir', payload })
}

export function beginDirectorySession(payload: BeginNavigationRequest) {
  return invokeCommand({
    command: 'begin_directory_session',
    payload,
  })
}

export function getDirectorySessionRange(payload: GetSessionRangeRequest) {
  return invokeCommand({
    command: 'get_directory_session_range',
    payload,
  })
}

export function reviseDirectorySessionView(payload: ReviseSessionViewRequest) {
  return invokeCommand({
    command: 'revise_directory_session_view',
    payload,
  })
}

export function releaseDirectorySession(payload: ReleaseSessionRequest) {
  return invokeCommand({
    command: 'release_directory_session',
    payload,
  })
}

export function listTreeChildren(payload: ListTreeChildrenRequest) {
  return invokeCommand({
    command: 'list_tree_children',
    payload,
  })
}

export function createFolder(payload: CreateEntryRequest) {
  return invokeCommand({ command: 'create_folder', payload })
}

export function createFile(payload: CreateEntryRequest) {
  return invokeCommand({ command: 'create_file', payload })
}

export function renameEntry(payload: RenameEntryRequest) {
  return invokeCommand({ command: 'rename_entry', payload })
}

export function moveToTrash(payload: TrashEntriesRequest) {
  return invokeCommand({ command: 'move_to_trash', payload })
}

export function listTrash() {
  return invokeCommand({ command: 'list_trash' })
}

export function restoreFromTrash(payload: RestoreTrashRequest) {
  return invokeCommand({ command: 'restore_from_trash', payload })
}

export function emptyTrash() {
  return invokeCommand({ command: 'empty_trash' })
}

export function deleteFromTrash(payload: DeleteFromTrashRequest) {
  return invokeCommand({ command: 'delete_from_trash', payload })
}

export function openPath(payload: OpenPathRequest) {
  return invokeCommand({ command: 'open_path', payload })
}

export function restartAsAdmin() {
  return invokeCommand({ command: 'restart_as_admin' })
}

export function setWindowTheme(payload: SetWindowThemeRequest) {
  return invokeCommand({ command: 'set_window_theme', payload })
}

export function getWindowBackdrop() {
  return invokeCommand({ command: 'get_window_backdrop' })
}

export function openFullDiskAccessSettings() {
  return invokeCommand({ command: 'open_full_disk_access_settings' })
}

export function startNativeDrag(payload: StartNativeDragRequest) {
  return invokeCommand({ command: 'start_native_drag', payload })
}

export function writeFileClipboard(payload: WriteFileClipboardRequest) {
  return invokeCommand({ command: 'write_file_clipboard', payload })
}

export function readFileClipboard() {
  return invokeCommand({ command: 'read_file_clipboard' })
}

export function clearFileClipboard() {
  return invokeCommand({ command: 'clear_file_clipboard' })
}

export function loadNativeMenu(payload: LoadNativeMenuRequest) {
  return invokeCommand({ command: 'load_native_menu', payload })
}

export function warmNativeMenus(payload: WarmNativeMenusRequest) {
  return invokeCommand({ command: 'warm_native_menus', payload })
}

export function invokeNativeMenuAction(payload: InvokeNativeMenuRequest) {
  return invokeCommand({
    command: 'invoke_native_menu_action',
    payload,
  })
}

export function showProperties(payload: ShowPropertiesRequest) {
  return invokeCommand({ command: 'show_properties', payload })
}

export function openWith(payload: OpenWithRequest) {
  return invokeCommand({ command: 'open_with', payload })
}

export function listApplications() {
  return invokeCommand({ command: 'list_applications' })
}

export function setDefaultApplication(payload: SetDefaultApplicationRequest) {
  return invokeCommand({ command: 'set_default_application', payload })
}

export function getDefaultApplication(payload: GetDefaultApplicationRequest) {
  return invokeCommand({
    command: 'get_default_application',
    payload,
  })
}

export function listVolumes() {
  return invokeCommand({ command: 'list_volumes' })
}

export function ejectVolume(payload: EjectVolumeRequest) {
  return invokeCommand({ command: 'eject_volume', payload })
}

export function folderSizeIndexStatus() {
  return invokeCommand({ command: 'folder_size_index_status' })
}

export function searchQuery(payload: SearchQueryRequest) {
  return invokeCommand({ command: 'search_query', payload })
}

export function searchPage(payload: SearchPageRequest) {
  return invokeCommand({ command: 'search_page', payload })
}

export function searchClose() {
  return invokeCommand({ command: 'search_close' })
}

export function setSizeSubscriptions(payload: SizeSubscriptionsRequest) {
  return invokeCommand({ command: 'set_size_subscriptions', payload })
}

export function requestFolderSize(payload: FolderSizeRequest) {
  return invokeCommand({ command: 'request_folder_size', payload })
}

export function requestFolderSizes(payload: FolderSizesRequest) {
  return invokeCommand({ command: 'request_folder_sizes', payload })
}

export function requestIcons(payload: RequestIconsRequest) {
  return invokeCommand({ command: 'request_icons', payload })
}

export function requestThumbnails(payload: RequestThumbnailsRequest) {
  return invokeCommand({
    command: 'request_thumbnails',
    payload,
  })
}

export function cancelThumbnails(payload: CancelThumbnailsRequest) {
  return invokeCommand({ command: 'cancel_thumbnails', payload })
}

export function requestVisibleItemCounts(payload: VisibleItemCountsRequest) {
  return invokeCommand({ command: 'request_visible_item_counts', payload })
}

export function sortActiveItems(payload: ActiveItemsSortRequest) {
  return invokeCommand({
    command: 'sort_active_items',
    payload,
  })
}

export function cancelSize(path: string) {
  return invokeCommand({ command: 'cancel_size', payload: { path } })
}

export function cancelSizes(paths: string[]) {
  return invokeCommand({
    command: 'cancel_sizes',
    payload: { paths },
  })
}

export function addIndexExclusion(paths: string[]) {
  return invokeCommand({ command: 'add_index_exclusion', payload: { paths } })
}

export function removeIndexExclusion(path: string) {
  return invokeCommand({ command: 'remove_index_exclusion', payload: { path } })
}

export function pickIndexExclusions() {
  return invokeCommand({ command: 'pick_index_exclusions' })
}

export function retryFolderSizeIndex() {
  return invokeCommand({ command: 'retry_folder_size_index' })
}

export function rebuildFolderSizeIndex() {
  return invokeCommand({ command: 'rebuild_folder_size_index' })
}

export function folderSizeIndexStorage() {
  return invokeCommand({ command: 'folder_size_index_storage' })
}

export function setTabWatch(
  target: WatchTarget | null,
  entries?: DirectoryEntry[],
  seedReference?: WatchSeedReference,
) {
  return invokeCommand({
    command: 'set_tab_watch',
    payload: { target, seedReference, entries },
  })
}

export function loadConfig() {
  return invokeCommand({ command: 'load_config' })
}

export function saveConfig(config: AppConfig) {
  return invokeCommand({ command: 'save_config', payload: { config } })
}

export function loadSession() {
  return invokeCommand({ command: 'load_session' })
}

export function saveSession(session: SessionState) {
  return invokeCommand({ command: 'save_session', payload: { session } })
}

export function readLogs() {
  return invokeCommand({ command: 'read_logs' })
}

export function setLogLevel(level: LogLevel) {
  return invokeCommand({ command: 'set_log_level', payload: { level } })
}
