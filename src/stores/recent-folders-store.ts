import { create } from 'zustand'
import { pathsMatch } from '@/lib/path-compare'

/** How many recently visited folders Search everywhere offers. */
export const MAX_RECENT_FOLDERS = 20

type RecentFoldersState = {
  /** Most recent first. */
  folders: string[]
  hydrate: (folders: string[]) => void
  record: (path: string) => void
}

export const useRecentFoldersStore = create<RecentFoldersState>((set) => ({
  folders: [],
  hydrate: (folders) => set({ folders: folders.slice(0, MAX_RECENT_FOLDERS) }),
  record: (path) =>
    set((state) => {
      if (state.folders[0] !== undefined && pathsMatch(state.folders[0], path)) {
        return state
      }
      return {
        folders: [path, ...state.folders.filter((folder) => !pathsMatch(folder, path))].slice(
          0,
          MAX_RECENT_FOLDERS,
        ),
      }
    }),
}))
