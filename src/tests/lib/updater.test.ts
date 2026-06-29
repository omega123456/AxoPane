import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  checkForAppUpdate,
  downloadAndInstallAppUpdate,
  getAppVersion,
  summarizeUpdate,
  type AppUpdate,
} from '@/lib/updater'

const relaunch = vi.fn(() => Promise.resolve())
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: () => relaunch() }))

async function withRuntime(userAgent: string, run: () => Promise<void>) {
  const win = window as unknown as Record<string, unknown>
  const internals = win.__TAURI_INTERNALS__
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
  const userAgentSpy = vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(userAgent)
  try {
    await run()
  } finally {
    userAgentSpy.mockRestore()
    if (internals === undefined) {
      delete win.__TAURI_INTERNALS__
    } else {
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        value: internals,
        configurable: true,
      })
    }
  }
}

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
  afterEach(() => {
    relaunch.mockClear()
  })

  it('relaunches after installing on a non-Windows Tauri runtime', async () => {
    const update = fakeUpdate()
    await withRuntime('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', async () => {
      expect(await downloadAndInstallAppUpdate(update)).toBe(true)
    })
    expect(update.downloadAndInstall).toHaveBeenCalledOnce()
    expect(relaunch).toHaveBeenCalledOnce()
  })

  it('does not relaunch after installing on a Windows Tauri runtime', async () => {
    const update = fakeUpdate()
    await withRuntime('Mozilla/5.0 (Windows NT 10.0; Win64; x64)', async () => {
      expect(await downloadAndInstallAppUpdate(update)).toBe(true)
    })
    expect(relaunch).not.toHaveBeenCalled()
  })

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

  it('falls back to the build version outside a Tauri runtime', async () => {
    expect(await getAppVersion()).toBe('0.1.0')
  })
})
