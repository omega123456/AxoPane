import { afterEach, beforeEach, vi } from 'vitest'
import { waitFor } from '@testing-library/react'
import { ipc } from '@/tests/ipc-mock'
import { invokeCommand, subscribeToEvent } from '@/lib/ipc/client'
import {
  cancelSize,
  everythingStatus,
  getInitialShell,
  listDir,
  listVolumes,
  loadConfig,
  loadSession,
  openPath,
  refreshTab,
  requestFolderSize,
  requestFolderSizes,
  saveConfig,
  saveSession,
  setTabWatch,
} from '@/lib/ipc/commands'
import {
  onDirPatch,
  onQueueConflict,
  onQueueProgress,
  onSizeState,
  onVolumesChanged,
  onWatchError,
} from '@/lib/ipc/events'
import { invokePlaywrightCommand, listenPlaywrightEvent } from '@/lib/ipc/playwright-ipc-mock'

beforeEach(() => {
  ipc.install()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ipc client + command wrappers (Tauri IPC bridge)', () => {
  it('invokes commands through the global bridge', async () => {
    await expect(getInitialShell()).resolves.toHaveProperty('panes')
    await expect(listVolumes()).resolves.toBeInstanceOf(Array)
    await expect(everythingStatus()).resolves.toHaveProperty('isAvailable')
    await expect(loadConfig()).resolves.toHaveProperty('theme')
    await expect(loadSession()).resolves.toHaveProperty('activePane')
  })

  it('passes payloads for request commands', async () => {
    ipc.override('list_dir', (payload) => ({ path: payload.path, entries: [] }))
    await expect(
      listDir({
        path: 'C:\\x',
        sortKey: 'name',
        sortDirection: 'asc',
        filter: '',
        showHidden: false,
        includeItemCounts: true,
      }),
    ).resolves.toEqual({ path: 'C:\\x', entries: [] })

    ipc.override('cancel_size', (payload) => ({ cancelled: payload.path === 'C:\\x' }))
    await expect(cancelSize('C:\\x')).resolves.toEqual({ cancelled: true })

    ipc.override('request_folder_size', () => undefined)
    ipc.override('request_folder_sizes', () => undefined)
    ipc.override('open_path', () => undefined)
    ipc.override('set_tab_watch', () => undefined)
    await expect(requestFolderSize({ path: 'C:\\x' })).resolves.toBeUndefined()
    await expect(requestFolderSizes({ paths: ['C:\\x'] })).resolves.toBeUndefined()
    await expect(openPath({ path: 'C:\\x' })).resolves.toBeUndefined()
    await expect(setTabWatch(null)).resolves.toBeUndefined()
    await expect(
      refreshTab({
        tabId: 't',
        path: 'C:\\x',
        sortKey: 'name',
        sortDirection: 'asc',
        filter: '',
        showHidden: false,
      }),
    ).resolves.toHaveProperty('changed')

    ipc.override('save_config', (payload) => payload.config)
    await expect(
      saveConfig({
        theme: 'light',
        showHiddenFiles: true,
        dismissedEverythingBanner: false,
        updateCheckInterval: '1d',
        logLevel: 'info',
        dateFormat: 'ymd',
        showTime: false,
        showSeconds: false,
        relativeDates: false,
        autoFolderSize: true,
        keybindings: {},
        columns: [],
        layout: {
          detailsVisible: false,
          treeWidthPx: 204,
          paneSplit: 0.5,
          columnWidths: {
            name: 320,
            size: 96,
            items: 72,
            type: 136,
            modified: 128,
            created: 128,
          },
          defaultPaneMode: 'dual',
          restoreSession: true,
          zoom: '100',
        },
      }),
    ).resolves.toHaveProperty('theme', 'light')

    ipc.override('save_session', (payload) => payload.session)
    await expect(
      saveSession({ activePane: 'left', leftPath: 'a', rightPath: 'b' }),
    ).resolves.toHaveProperty('activePane', 'left')
  })

  it('logs and rethrows when a command fails', async () => {
    ipc.override('list_dir', () => {
      throw new Error('boom')
    })

    await expect(
      listDir({
        path: 'C:\\x',
        sortKey: 'name',
        sortDirection: 'asc',
        filter: '',
        showHidden: false,
        includeItemCounts: true,
      }),
    ).rejects.toThrow('boom')
  })

  it('subscribes through the global bridge and cleans up listeners', async () => {
    const handler = vi.fn()
    const unlisten = await subscribeToEvent('dir://patch', handler)
    ipc.emit('dir://patch', {
      tabId: 't',
      path: 'p',
      reason: 'watch',
      changed: [],
      removed: [],
    })
    expect(handler).toHaveBeenCalledOnce()
    unlisten()
    ipc.emit('dir://patch', {
      tabId: 't',
      path: 'p',
      reason: 'watch',
      changed: [],
      removed: [],
    })
    expect(handler).toHaveBeenCalledOnce()
  })

  it('exposes typed event subscription helpers', async () => {
    const handlers = [
      await onDirPatch(vi.fn()),
      await onSizeState(vi.fn()),
      await onVolumesChanged(vi.fn()),
      await onQueueProgress(vi.fn()),
      await onQueueConflict(vi.fn()),
      await onWatchError(vi.fn()),
    ]
    for (const unlisten of handlers) {
      expect(typeof unlisten).toBe('function')
      unlisten()
    }
  })
})

describe('ipc client (Playwright web build fallback)', () => {
  beforeEach(() => {
    vi.unstubAllGlobals()
    vi.stubGlobal('__TAURI_IPC__', undefined)
    vi.stubEnv('VITE_PLAYWRIGHT', 'true')
  })

  afterEach(() => {
    vi.unstubAllEnvs()
  })

  it('routes invoke through the playwright mock', async () => {
    await expect(invokeCommand({ command: 'list_volumes' })).resolves.toBeInstanceOf(Array)
  })

  it('routes subscribe through the playwright mock', async () => {
    const handler = vi.fn()
    const unlisten = await subscribeToEvent('size://state', handler)
    expect(typeof unlisten).toBe('function')
    unlisten()
  })
})

describe('playwright ipc mock module', () => {
  afterEach(() => {
    delete (globalThis as { __PLAYWRIGHT_IPC_SCENARIO__?: unknown }).__PLAYWRIGHT_IPC_SCENARIO__
  })

  it('returns fixture responses and manages listeners', async () => {
    await expect(invokePlaywrightCommand('list_volumes', undefined)).resolves.toBeInstanceOf(Array)
    const handler = vi.fn()
    const unlisten = await listenPlaywrightEvent('dir://patch', handler)
    unlisten()
    expect(handler).not.toHaveBeenCalled()
  })

  it('uses scenario command overrides, command errors, and async delays', async () => {
    const overriddenVolumes = [
      {
        mountRoot: 'Z:\\',
        label: 'Archive',
        totalBytes: 100,
        freeBytes: 25,
        isNetwork: true,
        isRemovable: false,
      },
    ]
    ;(globalThis as { __PLAYWRIGHT_IPC_SCENARIO__?: unknown }).__PLAYWRIGHT_IPC_SCENARIO__ = {
      commands: { list_volumes: overriddenVolumes },
      commandErrors: { open_path: 'Cannot open this file' },
      delaysMs: { list_volumes: 1 },
    }

    await expect(invokePlaywrightCommand('list_volumes', undefined)).resolves.toEqual(
      overriddenVolumes,
    )
    await expect(invokePlaywrightCommand('open_path', { path: 'C:\\blocked.txt' })).rejects.toThrow(
      'Cannot open this file',
    )
  })

  it('replays scripted scenario events to all current listeners', async () => {
    ;(globalThis as { __PLAYWRIGHT_IPC_SCENARIO__?: unknown }).__PLAYWRIGHT_IPC_SCENARIO__ = {
      events: {
        'size://state': [
          {
            path: 'C:\\folder',
            state: 'ready',
            source: 'manual',
            sizeBytes: 42,
          },
        ],
      },
    }
    const first = vi.fn()
    const second = vi.fn()

    const unlistenFirst = await listenPlaywrightEvent('size://state', first)
    const unlistenSecond = await listenPlaywrightEvent('size://state', second)

    await waitFor(() => expect(first).toHaveBeenCalled())
    expect(second).toHaveBeenCalled()

    unlistenFirst()
    unlistenSecond()
  })
})
