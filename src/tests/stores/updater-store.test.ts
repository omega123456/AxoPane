import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppUpdate } from '@/lib/updater'

const checkForAppUpdate = vi.fn<() => Promise<AppUpdate | null>>()
const downloadAndInstallAppUpdate = vi.fn<(update?: AppUpdate | null) => Promise<boolean>>()

vi.mock('@/lib/updater', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/updater')>()
  return {
    ...actual,
    checkForAppUpdate: () => checkForAppUpdate(),
    downloadAndInstallAppUpdate: (update?: AppUpdate | null) => downloadAndInstallAppUpdate(update),
  }
})

const { useUpdaterStore } = await import('@/stores/updater-store')
const { useConfigStore } = await import('@/stores/config-store')

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

beforeEach(() => {
  vi.useFakeTimers()
  checkForAppUpdate.mockReset()
  downloadAndInstallAppUpdate.mockReset()
  downloadAndInstallAppUpdate.mockResolvedValue(true)
  useUpdaterStore.getState().stopPeriodicCheck()
  useUpdaterStore.getState().dismiss()
  useConfigStore.getState().reset()
})

afterEach(() => {
  useUpdaterStore.getState().stopPeriodicCheck()
  vi.useRealTimers()
})

describe('updater-store', () => {
  it('surfaces an available update from a manual check', async () => {
    const update = fakeUpdate()
    checkForAppUpdate.mockResolvedValue(update)

    await useUpdaterStore.getState().checkForUpdate(true)

    const state = useUpdaterStore.getState()
    expect(state.status).toBe('available')
    expect(state.update).toBe(update)
    expect(state.summary).toMatchObject({ version: '0.2.0', currentVersion: '0.1.0' })
  })

  it('reports up to date on a manual check then settles back to idle', async () => {
    checkForAppUpdate.mockResolvedValue(null)

    await useUpdaterStore.getState().checkForUpdate(true)
    expect(useUpdaterStore.getState().status).toBe('up-to-date')

    await vi.advanceTimersByTimeAsync(4_000)
    expect(useUpdaterStore.getState().status).toBe('idle')
  })

  it('stays idle when a background check finds nothing', async () => {
    checkForAppUpdate.mockResolvedValue(null)

    await useUpdaterStore.getState().checkForUpdate(false)
    expect(useUpdaterStore.getState().status).toBe('idle')
  })

  it('keeps an already-surfaced update when a background check returns null', async () => {
    const update = fakeUpdate()
    useUpdaterStore.getState().setAvailable(update, { currentVersion: '0.1.0', version: '0.2.0' })
    checkForAppUpdate.mockResolvedValue(null)

    await useUpdaterStore.getState().checkForUpdate(false)
    expect(useUpdaterStore.getState().status).toBe('available')
    expect(useUpdaterStore.getState().update).toBe(update)
  })

  it('surfaces an error from a manual check but swallows background failures', async () => {
    checkForAppUpdate.mockRejectedValue(new Error('offline'))

    await useUpdaterStore.getState().checkForUpdate(true)
    expect(useUpdaterStore.getState().status).toBe('error')
    expect(useUpdaterStore.getState().error).toBe('offline')

    useUpdaterStore.getState().dismiss()
    await useUpdaterStore.getState().checkForUpdate(false)
    expect(useUpdaterStore.getState().status).toBe('idle')
  })

  it('ignores a check while already checking or installing', async () => {
    useUpdaterStore.setState({ status: 'installing' })
    await useUpdaterStore.getState().checkForUpdate(true)
    expect(checkForAppUpdate).not.toHaveBeenCalled()
  })

  it('installs the available update', async () => {
    const update = fakeUpdate()
    useUpdaterStore.getState().setAvailable(update, { currentVersion: '0.1.0', version: '0.2.0' })

    await useUpdaterStore.getState().downloadAndInstall()
    expect(downloadAndInstallAppUpdate).toHaveBeenCalledWith(update)
  })

  it('reports an install failure', async () => {
    const update = fakeUpdate()
    useUpdaterStore.getState().setAvailable(update, { currentVersion: '0.1.0', version: '0.2.0' })
    downloadAndInstallAppUpdate.mockRejectedValue(new Error('disk full'))

    await useUpdaterStore.getState().downloadAndInstall()
    expect(useUpdaterStore.getState().status).toBe('error')
    expect(useUpdaterStore.getState().error).toBe('disk full')
  })

  it('does nothing when installing without an available update', async () => {
    await useUpdaterStore.getState().downloadAndInstall()
    expect(downloadAndInstallAppUpdate).not.toHaveBeenCalled()
  })

  it('checks once on launch and polls at the configured cadence', async () => {
    checkForAppUpdate.mockResolvedValue(null)
    useConfigStore.setState({ updateCheckInterval: '1h' })

    useUpdaterStore.getState().startPeriodicCheck()
    await vi.advanceTimersByTimeAsync(0)
    expect(checkForAppUpdate).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(3_600_000)
    expect(checkForAppUpdate).toHaveBeenCalledTimes(2)

    useUpdaterStore.getState().stopPeriodicCheck()
    await vi.advanceTimersByTimeAsync(3_600_000)
    expect(checkForAppUpdate).toHaveBeenCalledTimes(2)
  })

  it('checks once but does not schedule polling when cadence is off', async () => {
    checkForAppUpdate.mockResolvedValue(null)
    useConfigStore.setState({ updateCheckInterval: 'off' })

    useUpdaterStore.getState().restartPeriodicCheck()
    await vi.advanceTimersByTimeAsync(0)
    expect(checkForAppUpdate).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(604_800_000)
    expect(checkForAppUpdate).toHaveBeenCalledTimes(1)
  })

  it('exposes setStatus and dismiss helpers', () => {
    useUpdaterStore.getState().setStatus('error', 'boom')
    expect(useUpdaterStore.getState()).toMatchObject({ status: 'error', error: 'boom' })

    useUpdaterStore.getState().dismiss()
    expect(useUpdaterStore.getState()).toMatchObject({ status: 'idle', update: null, summary: null })
  })
})
