import type {
  AppConfig,
  CancelSizeResponse,
  CreateEntryRequest,
  DeleteEntriesRequest,
  DirectoryEntry,
  DirPatchEvent,
  EverythingStatus,
  FolderSizeRequest,
  FolderSizesRequest,
  InitialShellResponse,
  ListDirRequest,
  ListDirResponse,
  RenameEntryRequest,
  WatchTarget,
  SessionState,
  VolumeInfo,
} from '@/lib/types/ipc'
import { invokeCommand } from './client'

export function getInitialShell() {
  return invokeCommand({ command: 'get_initial_shell' }) as Promise<InitialShellResponse>
}

export function listDir(payload: ListDirRequest) {
  return invokeCommand({ command: 'list_dir', payload }) as Promise<ListDirResponse>
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

export function deleteEntries(payload: DeleteEntriesRequest) {
  return invokeCommand({ command: 'delete_entries', payload }) as Promise<void>
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
