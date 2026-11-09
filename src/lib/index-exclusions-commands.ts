import { log } from '@/lib/app-log-commands'
import {
  addIndexExclusion,
  pickIndexExclusions,
  removeIndexExclusion,
  retryFolderSizeIndex,
} from '@/lib/ipc/commands'
import type { IndexExclusionOutcome } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'

const NO_OUTCOMES: IndexExclusionOutcome[] = []
const NO_PICKED_FOLDERS: string[] = []

function storeExclusions(exclusions: string[]) {
  useConfigStore.getState().setIndexExclusions(exclusions)
}

export async function excludeFolders(paths: string[]): Promise<IndexExclusionOutcome[]> {
  try {
    const response = await addIndexExclusion(paths)
    storeExclusions(response.exclusions)
    return response.outcomes
  } catch (error) {
    log.warn('add_index_exclusion IPC failed', { paths, error })
    return NO_OUTCOMES
  }
}

export async function includeFolder(path: string): Promise<boolean> {
  try {
    const response = await removeIndexExclusion(path)
    storeExclusions(response.exclusions)
    return true
  } catch (error) {
    log.warn('remove_index_exclusion IPC failed', { path, error })
    return false
  }
}

export async function pickFoldersToExclude(): Promise<string[]> {
  try {
    const response = await pickIndexExclusions()
    return response.paths
  } catch (error) {
    log.warn('pick_index_exclusions IPC failed', { error })
    return NO_PICKED_FOLDERS
  }
}

export async function retryIndexStart() {
  try {
    await retryFolderSizeIndex()
  } catch (error) {
    log.warn('retry_folder_size_index IPC failed', { error })
  }
}
