import type { FolderSizeIndexStatus, ListDirResponse } from '@/lib/types/ipc'

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
