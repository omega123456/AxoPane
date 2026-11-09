import { create } from 'zustand'
import type { LogLevel, ThemePreference } from '@/lib/types/ipc'
import { DEFAULT_DATE_FORMAT, type DateFormat } from '@/lib/date-format'
import { DEFAULT_UPDATE_INTERVAL, type UpdateInterval } from '@/lib/update-intervals'
import { persistAppConfig } from '@/lib/app-config'
import { setLogLevel as applyAndPersistBackendLogLevel } from '@/lib/ipc/commands'
import {
  withAddedFavourite,
  withoutFavourite,
  withRenamedFavourite,
  withReorderedFavourite,
} from '@/stores/config/favourites'

export type ConfigSnapshot = {
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

export function defaultConfigSnapshot(): ConfigSnapshot {
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

const CONFIG_SNAPSHOT_FIELDS = Object.keys(defaultConfigSnapshot()) as (keyof ConfigSnapshot)[]

/** The config-store fields of `source`, each falling back to its default when nullish. */
export function pickConfigSnapshot(source: Partial<ConfigSnapshot>): ConfigSnapshot {
  const defaults = defaultConfigSnapshot()
  return Object.fromEntries(
    CONFIG_SNAPSHOT_FIELDS.map((field) => [field, source[field] ?? defaults[field]]),
  ) as ConfigSnapshot
}

export function configFieldPatch<Field extends keyof ConfigSnapshot>(
  field: Field,
  value: ConfigSnapshot[Field],
): Pick<ConfigSnapshot, Field> {
  return { [field]: value } as Pick<ConfigSnapshot, Field>
}

export const useConfigStore = create<ConfigStore>((set, get) => {
  const setAndPersist = async (patch: Partial<ConfigStore>) => {
    set(patch)
    await persistAppConfig()
  }

  const persistField =
    <Field extends keyof ConfigSnapshot>(field: Field) =>
    (value: ConfigSnapshot[Field]) =>
      setAndPersist(configFieldPatch(field, value))

  const commitChange = async (patch: Partial<ConfigStore> | null) => {
    if (!patch) {
      return
    }
    await setAndPersist(patch)
  }

  return {
    ...defaultConfigSnapshot(),
    renamingFavourite: null,
    hydrate: (config) => set(config),
    setThemePreference: persistField('theme'),
    setShowHiddenFiles: persistField('showHiddenFiles'),
    setUpdateCheckInterval: persistField('updateCheckInterval'),
    setDateFormat: persistField('dateFormat'),
    setShowTime: persistField('showTime'),
    setShowSeconds: persistField('showSeconds'),
    setRelativeDates: persistField('relativeDates'),
    setAutoFolderSize: persistField('autoFolderSize'),
    setAutoExpandActiveQueueToasts: persistField('autoExpandActiveQueueToasts'),
    setFavouritesCollapsed: persistField('favouritesCollapsed'),
    setLogLevel: async (logLevel) => {
      set({ logLevel })
      await applyAndPersistBackendLogLevel(logLevel)
    },
    dismissEverythingBanner: () => setAndPersist({ dismissedEverythingBanner: true }),
    addFavourite: (path, index) => commitChange(withAddedFavourite(get(), path, index)),
    removeFavourite: (path) => commitChange(withoutFavourite(get(), path)),
    setRenamingFavourite: (renamingFavourite) => set({ renamingFavourite }),
    renameFavourite: async (path, name) => {
      const renamed = withRenamedFavourite(get(), path, name)
      set({ renamingFavourite: null, ...renamed })
      if (renamed) {
        await persistAppConfig()
      }
    },
    reorderFavourite: (path, index) => commitChange(withReorderedFavourite(get(), path, index)),
    reset: () => set({ ...defaultConfigSnapshot(), renamingFavourite: null }),
  }
})
