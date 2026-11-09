import { create } from 'zustand'
import { pathsMatch } from '@/lib/path-compare'

export const MAX_RECENT_FOLDERS = 20

type RecentFoldersState = {
  folders: string[]
  hydrate: (folders: string[]) => void
  record: (path: string) => void
}

function withRecordedFolder(folders: string[], path: string) {
  const alreadyMostRecent = folders[0] !== undefined && pathsMatch(folders[0], path)
  if (alreadyMostRecent) {
    return null
  }
  const otherFolders = folders.filter((folder) => !pathsMatch(folder, path))
  return { folders: [path, ...otherFolders].slice(0, MAX_RECENT_FOLDERS) }
}

export const useRecentFoldersStore = create<RecentFoldersState>((set) => ({
  folders: [],
  hydrate: (folders) => set({ folders: folders.slice(0, MAX_RECENT_FOLDERS) }),
  record: (path) => set((state) => withRecordedFolder(state.folders, path) ?? state),
}))
