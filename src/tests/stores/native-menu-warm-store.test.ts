import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { useNativeMenuWarmStore } from '@/stores/native-menu-warm-store'
import { usePanesStore } from '@/stores/panes-store'
import { TRASH_PATH } from '@/lib/trash'
import type { DirectoryEntry, WarmNativeMenusRequest } from '@/lib/types/ipc'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

function entryAt(path: string, isDir = false): DirectoryEntry {
  const name = path.split(/[/\\]/).filter(Boolean).at(-1) ?? path
  return {
    id: path,
    name,
    path,
    isDir,
    iconDataUrl: null,
    sizeBytes: isDir ? null : 100,
    itemCount: isDir ? 0 : null,
    typeLabel: isDir ? 'Folder' : 'File',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

function setPaneEntries(path: string, entries: DirectoryEntry[]) {
  usePanesStore.setState((state) => ({
    panes: {
      ...state.panes,
      left: { ...state.panes.left, path, entries },
    },
  }))
}

beforeEach(() => {
  setPlatform('Win32')
  useNativeMenuWarmStore.getState().resetWarmedTypeKeys()
  setPaneEntries('C:\\root', [])
})

afterEach(() => {
  setPlatform(originalPlatform)
})

describe('warmVisibleNativeMenus', () => {
  it('fires one warm request per distinct un-warmed type among visible entries', async () => {
    const warm = vi.fn<(payload: WarmNativeMenusRequest) => void>(() => undefined)
    ipc.override('warm_native_menus', warm)
    setPaneEntries('C:\\root', [
      entryAt('C:\\root\\a.pdf'),
      entryAt('C:\\root\\b.pdf'),
      entryAt('C:\\root\\Documents', true),
    ])

    await useNativeMenuWarmStore
      .getState()
      .warmVisibleNativeMenus('left', ['C:\\root\\a.pdf', 'C:\\root\\b.pdf', 'C:\\root\\Documents'])

    expect(warm).toHaveBeenCalledTimes(1)
    const payload = warm.mock.calls[0]?.[0]
    expect(payload).toBeDefined()
    expect(payload?.requests).toHaveLength(2)
    expect(payload?.requests).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ targetKind: 'file', selectedPaths: ['C:\\root\\a.pdf'] }),
        expect.objectContaining({ targetKind: 'folder', selectedPaths: ['C:\\root\\Documents'] }),
      ]),
    )
  })

  it('skips types already warmed this session and dedupes duplicates within a batch', async () => {
    const warm = vi.fn<(payload: WarmNativeMenusRequest) => void>(() => undefined)
    ipc.override('warm_native_menus', warm)
    setPaneEntries('C:\\root', [entryAt('C:\\root\\a.pdf'), entryAt('C:\\root\\b.pdf')])

    await useNativeMenuWarmStore
      .getState()
      .warmVisibleNativeMenus('left', ['C:\\root\\a.pdf', 'C:\\root\\a.pdf', 'C:\\root\\b.pdf'])
    expect(warm).toHaveBeenCalledTimes(1)
    expect(warm.mock.calls[0]?.[0]?.requests).toHaveLength(1)

    warm.mockClear()
    await useNativeMenuWarmStore
      .getState()
      .warmVisibleNativeMenus('left', ['C:\\root\\a.pdf', 'C:\\root\\b.pdf'])
    expect(warm).not.toHaveBeenCalled()
  })

  it('marks types warmed optimistically before the IPC call resolves', () => {
    ipc.override('warm_native_menus', () => undefined)
    setPaneEntries('C:\\root', [entryAt('C:\\root\\a.pdf')])

    const promise = useNativeMenuWarmStore
      .getState()
      .warmVisibleNativeMenus('left', ['C:\\root\\a.pdf'])
    expect(useNativeMenuWarmStore.getState().warmedTypeKeys['file::pdf']).toBe(true)

    return promise
  })

  it('rolls back the batch keys on a transport error, allowing a later retry', async () => {
    const warm = vi.fn<(payload: WarmNativeMenusRequest) => void>(() => {
      throw new Error('transport failure')
    })
    ipc.override('warm_native_menus', warm)
    setPaneEntries('C:\\root', [entryAt('C:\\root\\a.pdf')])

    await expect(
      useNativeMenuWarmStore.getState().warmVisibleNativeMenus('left', ['C:\\root\\a.pdf']),
    ).rejects.toThrow('transport failure')
    expect(useNativeMenuWarmStore.getState().warmedTypeKeys['file::pdf']).toBeUndefined()

    warm.mockReset()
    warm.mockImplementation(() => undefined)
    await useNativeMenuWarmStore.getState().warmVisibleNativeMenus('left', ['C:\\root\\a.pdf'])
    expect(warm).toHaveBeenCalledTimes(1)
    expect(useNativeMenuWarmStore.getState().warmedTypeKeys['file::pdf']).toBe(true)
  })

  it('issues zero warm requests on non-Windows platforms', async () => {
    setPlatform('MacIntel')
    const warm = vi.fn(() => undefined)
    ipc.override('warm_native_menus', warm)
    setPaneEntries('C:\\root', [entryAt('C:\\root\\a.pdf')])

    await useNativeMenuWarmStore.getState().warmVisibleNativeMenus('left', ['C:\\root\\a.pdf'])
    expect(warm).not.toHaveBeenCalled()
  })

  it('issues zero warm requests for a trash pane', async () => {
    const warm = vi.fn(() => undefined)
    ipc.override('warm_native_menus', warm)
    setPaneEntries(TRASH_PATH, [entryAt('C:\\root\\a.pdf')])

    await useNativeMenuWarmStore.getState().warmVisibleNativeMenus('left', ['C:\\root\\a.pdf'])
    expect(warm).not.toHaveBeenCalled()
  })

  it('ignores paths that no longer resolve to an entry in the pane', async () => {
    const warm = vi.fn(() => undefined)
    ipc.override('warm_native_menus', warm)
    setPaneEntries('C:\\root', [entryAt('C:\\root\\a.pdf')])

    await useNativeMenuWarmStore
      .getState()
      .warmVisibleNativeMenus('left', ['C:\\root\\missing.pdf'])
    expect(warm).not.toHaveBeenCalled()
  })
})
