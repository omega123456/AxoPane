import type {
  FolderSizeIndexStatus,
  ListDirResponse,
  MenuActionStatus,
  VolumeInfo,
} from '@/lib/types/ipc'

/**
 * Fixture payloads for the Phase 5 directory state screenshots. Specs select
 * one of these via `window.__PLAYWRIGHT_IPC_OVERRIDES__` so the screenshot
 * router stays free of inline domain data.
 */
export const emptyListDir: ListDirResponse = {
  path: 'C:\\Users\\Omega',
  entries: [],
}

export const everythingUnavailable: FolderSizeIndexStatus = {
  backend: 'everything',
  isAvailable: false,
  state: 'unavailable',
}

export const everythingAvailable: FolderSizeIndexStatus = {
  backend: 'everything',
  isAvailable: true,
  state: 'upToDate',
}

/** No folder-size backend (macOS without the index). */
export const noFolderSizeBackend: FolderSizeIndexStatus = {
  backend: 'none',
  isAvailable: false,
  state: 'unavailable',
}

/** macOS index statuses, one per Settings status line. */
export const macIndexStatuses = {
  notReady: { backend: 'macIndex', isAvailable: true, state: 'notReady' },
  indexing: { backend: 'macIndex', isAvailable: true, state: 'indexing', itemsScanned: 1_234_567 },
  updating: { backend: 'macIndex', isAvailable: true, state: 'updating' },
  upToDate: { backend: 'macIndex', isAvailable: true, state: 'upToDate' },
  pausedLowPower: {
    backend: 'macIndex',
    isAvailable: true,
    state: 'paused',
    pauseReason: 'lowPowerMode',
  },
  pausedThermal: {
    backend: 'macIndex',
    isAvailable: true,
    state: 'paused',
    pauseReason: 'thermal',
  },
} satisfies Record<string, FolderSizeIndexStatus>

/**
 * Drives at each capacity level: under 85% used (accent), 91% (warning) and
 * 96% (danger). Network shares get no bar.
 */
export const capacityLevelVolumes: VolumeInfo[] = [
  {
    mountRoot: 'C:\\',
    label: 'Windows',
    totalBytes: 1_000_000_000,
    freeBytes: 500_000_000,
    isNetwork: false,
    isRemovable: false,
  },
  {
    mountRoot: 'D:\\',
    label: 'Projects',
    totalBytes: 1_000_000_000,
    freeBytes: 90_000_000,
    isNetwork: false,
    isRemovable: false,
  },
  {
    mountRoot: 'E:\\',
    label: 'USB Stick',
    totalBytes: 64_000_000,
    freeBytes: 2_560_000,
    isNetwork: false,
    isRemovable: true,
  },
  {
    mountRoot: 'Z:\\',
    label: 'Share',
    totalBytes: 500_000_000,
    freeBytes: 200_000_000,
    isNetwork: true,
    isRemovable: false,
  },
]

/** An eject the OS refused, which surfaces the floating error InfoBar. */
export const ejectRefused: MenuActionStatus = {
  handled: false,
  message: 'USB Stick (E:) is in use. Close any open files and try again.',
}
