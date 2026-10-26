import { create } from 'zustand'
import type { LogLevel, ThemePreference } from '@/lib/types/ipc'
import { DEFAULT_DATE_FORMAT, type DateFormat } from '@/lib/date-format'
import { DEFAULT_UPDATE_INTERVAL, type UpdateInterval } from '@/lib/update-intervals'
import { persistAppConfig } from '@/lib/app-config'
import { setLogLevel as setBackendLogLevel } from '@/lib/ipc/commands'
import { pathsMatch } from '@/lib/path-compare'

type ConfigSnapshot = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  updateCheckInterval: UpdateInterval
  logLevel: LogLevel
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
  relativeDates: boolean
  autoFolderSize: boolean
  autoExpandActiveQueueToasts: boolean
  favourites: string[]
  favouriteNames: Record<string, string>
  favouritesCollapsed: boolean
}

type ConfigStore = ConfigSnapshot & {
  /** Favourite path whose name is being edited inline; never persisted. */
  renamingFavourite: string | null
  hydrate: (config: ConfigSnapshot) => void
  setThemePreference: (theme: ThemePreference) => Promise<void>
  setShowHiddenFiles: (showHiddenFiles: boolean) => Promise<void>
  setUpdateCheckInterval: (updateCheckInterval: UpdateInterval) => Promise<void>
  setLogLevel: (logLevel: LogLevel) => Promise<void>
  setDateFormat: (dateFormat: DateFormat) => Promise<void>
  setShowTime: (showTime: boolean) => Promise<void>
  setShowSeconds: (showSeconds: boolean) => Promise<void>
  setRelativeDates: (relativeDates: boolean) => Promise<void>
  setAutoFolderSize: (autoFolderSize: boolean) => Promise<void>
  setAutoExpandActiveQueueToasts: (autoExpandActiveQueueToasts: boolean) => Promise<void>
  dismissEverythingBanner: () => Promise<void>
  addFavourite: (path: string, index?: number) => Promise<void>
  removeFavourite: (path: string) => Promise<void>
  reorderFavourite: (path: string, index: number) => Promise<void>
  setRenamingFavourite: (path: string | null) => void
  /** A blank name restores the folder's own name. */
  renameFavourite: (path: string, name: string) => Promise<void>
  setFavouritesCollapsed: (favouritesCollapsed: boolean) => Promise<void>
  reset: () => void
}

function defaultState(): ConfigSnapshot {
  return {
    theme: 'system',
    showHiddenFiles: false,
    dismissedEverythingBanner: false,
    updateCheckInterval: DEFAULT_UPDATE_INTERVAL,
    logLevel: 'info',
    dateFormat: DEFAULT_DATE_FORMAT,
    showTime: false,
    showSeconds: false,
    relativeDates: false,
    autoFolderSize: true,
    autoExpandActiveQueueToasts: false,
    favourites: [],
    favouriteNames: {},
    favouritesCollapsed: false,
  }
}

export const useConfigStore = create<ConfigStore>((set) => ({
  ...defaultState(),
  renamingFavourite: null,
  hydrate: (config) => set(config),
  setThemePreference: async (theme) => {
    set({ theme })
    await persistAppConfig()
  },
  setShowHiddenFiles: async (showHiddenFiles) => {
    set({ showHiddenFiles })
    await persistAppConfig()
  },
  setUpdateCheckInterval: async (updateCheckInterval) => {
    set({ updateCheckInterval })
    await persistAppConfig()
  },
  setDateFormat: async (dateFormat) => {
    set({ dateFormat })
    await persistAppConfig()
  },
  setShowTime: async (showTime) => {
    set({ showTime })
    await persistAppConfig()
  },
  setShowSeconds: async (showSeconds) => {
    set({ showSeconds })
    await persistAppConfig()
  },
  setRelativeDates: async (relativeDates) => {
    set({ relativeDates })
    await persistAppConfig()
  },
  setAutoFolderSize: async (autoFolderSize) => {
    set({ autoFolderSize })
    await persistAppConfig()
  },
  setAutoExpandActiveQueueToasts: async (autoExpandActiveQueueToasts) => {
    set({ autoExpandActiveQueueToasts })
    await persistAppConfig()
  },
  setFavouritesCollapsed: async (favouritesCollapsed) => {
    set({ favouritesCollapsed })
    await persistAppConfig()
  },
  setLogLevel: async (logLevel) => {
    set({ logLevel })
    // Applies to the running backend logger immediately and persists the level
    // into the app config (config.json), so capture survives restarts.
    await setBackendLogLevel(logLevel)
  },
  dismissEverythingBanner: async () => {
    set({ dismissedEverythingBanner: true })
    await persistAppConfig()
  },
  addFavourite: async (path, index) => {
    let changed = false
    set((state) => {
      if (state.favourites.some((item) => pathsMatch(item, path))) return state
      const favourites = [...state.favourites]
      favourites.splice(
        Math.max(0, Math.min(index ?? favourites.length, favourites.length)),
        0,
        path,
      )
      changed = true
      return { favourites }
    })
    if (changed) await persistAppConfig()
  },
  removeFavourite: async (path) => {
    let changed = false
    set((state) => {
      const favourites = state.favourites.filter((item) => !pathsMatch(item, path))
      changed = favourites.length !== state.favourites.length
      const favouriteNames = Object.fromEntries(
        Object.entries(state.favouriteNames).filter(([key]) => favourites.includes(key)),
      )
      return changed ? { favourites, favouriteNames } : state
    })
    if (changed) await persistAppConfig()
  },
  setRenamingFavourite: (renamingFavourite) => set({ renamingFavourite }),
  renameFavourite: async (path, name) => {
    let changed = false
    set((state) => {
      const trimmed = name.trim()
      const { [path]: previous, ...rest } = state.favouriteNames
      changed = (trimmed || undefined) !== previous
      const favouriteNames = trimmed ? { ...rest, [path]: trimmed } : rest
      return { renamingFavourite: null, ...(changed ? { favouriteNames } : {}) }
    })
    if (changed) await persistAppConfig()
  },
  reorderFavourite: async (path, index) => {
    let changed = false
    set((state) => {
      const sourceIndex = state.favourites.findIndex((item) => pathsMatch(item, path))
      if (sourceIndex === -1) return state
      const favourites = [...state.favourites]
      const [item] = favourites.splice(sourceIndex, 1)
      const adjustedIndex = sourceIndex < index ? index - 1 : index
      const targetIndex = Math.max(0, Math.min(adjustedIndex, favourites.length))
      favourites.splice(targetIndex, 0, item)
      changed = sourceIndex !== targetIndex
      return changed ? { favourites } : state
    })
    if (changed) await persistAppConfig()
  },
  reset: () => set({ ...defaultState(), renamingFavourite: null }),
}))
