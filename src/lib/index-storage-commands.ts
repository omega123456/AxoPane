import { log } from '@/lib/app-log-commands'
import { folderSizeIndexStorage, rebuildFolderSizeIndex } from '@/lib/ipc/commands'
import type { FolderSizeIndexStorage } from '@/lib/types/ipc'

export async function loadIndexStorage(): Promise<FolderSizeIndexStorage | null> {
  try {
    return await folderSizeIndexStorage()
  } catch (error) {
    log.warn('folder_size_index_storage IPC failed', { error })
    return null
  }
}

export async function rebuildIndex() {
  try {
    await rebuildFolderSizeIndex()
  } catch (error) {
    log.warn('rebuild_folder_size_index IPC failed', { error })
  }
}
