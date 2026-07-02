import type {
  AppConfig,
  CancelSizeResponse,
  CompressArchiveRequest,
  CreateEntryRequest,
  DeleteFromTrashRequest,
  DirectoryEntry,
  DirPatchEvent,
  EverythingStatus,
  ExtractArchiveRequest,
  FolderSizeRequest,
  FolderSizesRequest,
  GetDefaultApplicationRequest,
  GetDefaultApplicationResponse,
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
  RenameEntryRequest,
  RequestIconsRequest,
  RestoreTrashRequest,
  SetDefaultApplicationRequest,
  ShowPropertiesRequest,
  TrashEntriesRequest,
  WarmNativeMenusRequest,
  WatchTarget,
  SessionState,
  VolumeInfo,
  WriteFileClipboardRequest,
} from '@/lib/types/ipc'
import { invokeCommand } from './client'

export function getInitialShell() {
  return invokeCommand({ command: 'get_initial_shell' }) as Promise<InitialShellResponse>
}

export function listDir(payload: ListDirRequest) {
  return invokeCommand({ command: 'list_dir', payload }) as Promise<ListDirResponse>
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

export function compressArchive(payload: CompressArchiveRequest) {
  return invokeCommand({ command: 'compress_archive', payload }) as Promise<MenuActionStatus>
}

export function extractArchive(payload: ExtractArchiveRequest) {
  return invokeCommand({ command: 'extract_archive', payload }) as Promise<MenuActionStatus>
}

export function listVolumes() {
  return invokeCommand({ command: 'list_volumes' }) as Promise<VolumeInfo[]>
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

export function cancelSize(path: string) {
  return invokeCommand({ command: 'cancel_size', payload: { path } }) as Promise<CancelSizeResponse>
}

export function setTabWatch(target: WatchTarget | null) {
  return invokeCommand({ command: 'set_tab_watch', payload: { target } }) as Promise<void>
}

export function refreshTab(target: WatchTarget) {
  return invokeCommand({ command: 'refresh_tab', payload: { target } }) as Promise<DirPatchEvent>
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
