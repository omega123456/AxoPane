import type { EverythingStatus, ListDirResponse } from '@/lib/types/ipc'

/**
 * Fixture payloads for the Phase 5 directory state screenshots. Specs select
 * one of these via `window.__PLAYWRIGHT_IPC_OVERRIDES__` so the screenshot
 * router stays free of inline domain data.
 */
export const emptyListDir: ListDirResponse = {
  path: 'C:\\Users\\Omega',
  entries: [],
}

export const everythingUnavailable: EverythingStatus = {
  status: 'unavailable',
  isAvailable: false,
}

export const everythingAvailable: EverythingStatus = {
  status: 'available',
  isAvailable: true,
}
