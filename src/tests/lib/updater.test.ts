import { describe, expect, it, vi } from 'vitest'
import {
  checkForAppUpdate,
  downloadAndInstallAppUpdate,
  summarizeUpdate,
  type AppUpdate,
} from '@/lib/updater'

function fakeUpdate(overrides: Partial<AppUpdate> = {}): AppUpdate {
  return {
    currentVersion: '0.1.0',
    version: '0.2.0',
    body: 'Fixes',
    date: '2026-06-24',
    downloadAndInstall: vi.fn(() => Promise.resolve()),
    ...overrides,
  }
}

describe('updater', () => {
  it('summarizes an update handle', () => {
    expect(summarizeUpdate(fakeUpdate())).toEqual({
      currentVersion: '0.1.0',
      version: '0.2.0',
      notes: 'Fixes',
      date: '2026-06-24',
    })
  })

  it('does not use the native updater outside a Tauri Windows runtime', async () => {
    // jsdom has no `__TAURI_INTERNALS__`, so the native updater is unavailable.
    expect(await checkForAppUpdate()).toBeNull()
    // With nothing to install, the apply flow reports false instead of throwing.
    expect(await downloadAndInstallAppUpdate()).toBe(false)
  })

  it('applies a provided update handle', async () => {
    const update = fakeUpdate()
    expect(await downloadAndInstallAppUpdate(update)).toBe(true)
    expect(update.downloadAndInstall).toHaveBeenCalledOnce()
  })
})
