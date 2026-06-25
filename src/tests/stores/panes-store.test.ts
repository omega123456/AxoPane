import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { getParentPath, schedulePersistSession, usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { DirectoryEntry, ListDirRequest, ListDirResponse, SaveConfigRequest } from '@/lib/types/ipc'

function dir(name: string, isDir = true): DirectoryEntry {
  return {
    id: name,
    name,
    path: `C:\\root\\${name}`,
    isDir,
    sizeBytes: isDir ? null : 100,
    itemCount: isDir ? 1 : null,
    typeLabel: isDir ? 'Folder' : 'File',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

function responder(payload: ListDirRequest): ListDirResponse {
  if (payload.path === 'C:\\fail') {
    throw new Error('Access is denied')
  }
  return { path: payload.path, entries: [dir('Alpha'), dir('Beta', false)] }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  ipc.override('list_dir', responder)
  ipc.override('set_tab_watch', () => undefined)
  ipc.override('save_session', (payload) => payload.session)
})

describe('panes-store navigation', () => {
  it('navigates, sorts, and records an error', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().panes.left.entries.map((e) => e.name)).toEqual(['Alpha', 'Beta'])

    await usePanesStore.getState().setSort('left', 'name')
    expect(usePanesStore.getState().panes.left.sortDirection).toBe('desc')

    await usePanesStore.getState().setSort('left', 'size')
    expect(usePanesStore.getState().panes.left.sortKey).toBe('size')

    await usePanesStore.getState().navigatePane('left', 'C:\\fail')
    expect(usePanesStore.getState().panes.left.error).toBe('Access is denied')
  })

  it('goes up to the parent path', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    await usePanesStore.getState().goUp('left')
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root')
  })

  it('debounces filter draft then applies it', async () => {
    vi.useFakeTimers()
    try {
      await usePanesStore.getState().navigatePane('left', 'C:\\root')
      usePanesStore.getState().setFilterDraft('left', 'Al')
      expect(usePanesStore.getState().panes.left.typing).toBe(true)
      await vi.advanceTimersByTimeAsync(250)
      expect(usePanesStore.getState().panes.left.filterApplied).toBe('Al')
      expect(usePanesStore.getState().panes.left.typing).toBe(false)

      usePanesStore.getState().setFilterDraft('left', 'X')
      usePanesStore.getState().clearFilter('left')
      expect(usePanesStore.getState().panes.left.filterApplied).toBe('')
    } finally {
      vi.useRealTimers()
    }
  })

  it('requests a manual size for a focused folder only', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_size', request)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    await usePanesStore.getState().requestManualSize('left', 'Beta') // a file -> ignored
    expect(request).not.toHaveBeenCalled()

    await usePanesStore.getState().requestManualSize('left', 'Alpha')
    expect(request).toHaveBeenCalledOnce()
  })

  it('applies size-state events and re-sorts when sorting by size', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().setSort('left', 'size')

    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Alpha',
      state: 'ready',
      source: 'everything',
      sizeBytes: 9999,
    })

    expect(usePanesStore.getState().sizeStates['C:\\root\\Alpha'].sizeBytes).toBe(9999)
  })

  it('refreshes everything by clearing size states then reloading', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Alpha',
      state: 'ready',
      source: 'everything',
      sizeBytes: 5,
    })
    await usePanesStore.getState().refreshEverything('left')
    expect(usePanesStore.getState().sizeStates['C:\\root\\Alpha']).toBeUndefined()
  })

  it('opens, switches with recheck, and closes tabs', async () => {
    ipc.override('refresh_tab', (payload) => ({
      tabId: payload.target.tabId,
      path: payload.target.path,
      reason: 'refresh',
      changed: [],
      removed: [],
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().openTabFromPath('left', 'C:\\root\\Alpha')
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(2)
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root\\Alpha')

    const firstTabId = useTabsStore.getState().panes.left.tabs[0].id
    await usePanesStore.getState().switchTab('left', firstTabId)
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)

    // switching to the already-active tab is a no-op activation
    await usePanesStore.getState().switchTab('left', firstTabId)
    expect(usePanesStore.getState().activePaneId).toBe('left')

    const secondTabId = useTabsStore.getState().panes.left.tabs[1].id
    await usePanesStore.getState().closeTab('left', secondTabId)
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
  })

  it('ignores a dir patch for a non-current path', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    const before = usePanesStore.getState().panes.left.entries
    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\elsewhere',
      reason: 'watch',
      changed: [{ path: 'C:\\elsewhere\\x', entry: dir('x') }],
      removed: [],
    })
    expect(usePanesStore.getState().panes.left.entries).toBe(before)
  })

  it('builds tree children and tolerates listing failures', async () => {
    await usePanesStore.getState().ensureTreeChildren('C:\\root')
    expect(usePanesStore.getState().treeNodes['C:\\root'].loaded).toBe(true)

    await usePanesStore.getState().ensureTreeChildren('C:\\fail')
    expect(usePanesStore.getState().treeNodes['C:\\fail'].loaded).toBe(true)
    expect(usePanesStore.getState().treeNodes['C:\\fail'].children).toEqual([])

    await usePanesStore.getState().toggleTreeNode('C:\\root')
    expect(usePanesStore.getState().treeNodes['C:\\root'].expanded).toBe(false)
  })

  it('eagerly requests folder sizes on reload only when Everything is available', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)

    // Everything unavailable (macOS / no-Everything Windows): sizes stay manual,
    // so reload must NOT auto-request them.
    usePanesStore.setState({ everythingStatus: { status: 'unavailable', isAvailable: false } })
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(request).not.toHaveBeenCalled()

    // Everything available (Windows): reload eagerly requests the visible dataset.
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(request).toHaveBeenCalledWith({ paths: ['C:\\root\\Alpha'] })
  })

  it('requests sizes for newly visible folders only when Everything is available', async () => {
    const request = vi.fn(() => undefined)
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    ipc.override('request_folder_sizes', request)

    // A range change that includes the directory entry triggers a fresh request
    // for folders without a recorded size state.
    usePanesStore.getState().setVisibleRange('left', 0, 5)
    expect(request).toHaveBeenCalledWith({ paths: ['C:\\root\\Alpha'] })

    // Re-emitting the same range is a no-op.
    request.mockClear()
    usePanesStore.getState().setVisibleRange('left', 0, 5)
    expect(request).not.toHaveBeenCalled()

    // With Everything unavailable, scrolling never auto-requests sizes.
    usePanesStore.setState({ everythingStatus: { status: 'unavailable', isAvailable: false } })
    usePanesStore.getState().setVisibleRange('left', 0, 8)
    expect(request).not.toHaveBeenCalled()
  })

  it('toggles hidden files, syncing config and reloading both panes', async () => {
    const saveConfig = vi.fn((payload: SaveConfigRequest) => payload.config)
    ipc.override('save_config', saveConfig)
    const listDir = vi.fn(responder)
    ipc.override('list_dir', listDir)

    expect(usePanesStore.getState().showHiddenFiles).toBe(false)
    await usePanesStore.getState().setShowHiddenFiles(true)

    expect(usePanesStore.getState().showHiddenFiles).toBe(true)
    expect(saveConfig).toHaveBeenCalled()
    // Both panes reloaded with the new visibility flag.
    expect(listDir).toHaveBeenCalledWith(expect.objectContaining({ showHidden: true }))
    expect(listDir.mock.calls.length).toBeGreaterThanOrEqual(2)

    // Setting the same value again is a no-op (no extra reloads).
    listDir.mockClear()
    await usePanesStore.getState().setShowHiddenFiles(true)
    expect(listDir).not.toHaveBeenCalled()
  })

  it('computes parent paths, preserving Windows drive roots', () => {
    expect(getParentPath('C:\\Users\\Omega')).toBe('C:\\Users')
    expect(getParentPath('C:\\Users')).toBe('C:\\')
    expect(getParentPath('C:\\')).toBeNull()
    expect(getParentPath('C:')).toBeNull()
    expect(getParentPath('/home/omega')).toBe('/home')
    expect(getParentPath('/home')).toBe('/')
    expect(getParentPath('/')).toBeNull()
    expect(getParentPath('')).toBeNull()
  })

  it('persists the session on a delay', async () => {
    vi.useFakeTimers()
    try {
      const save = vi.fn((payload) => payload.session)
      ipc.override('save_session', save)
      schedulePersistSession('left')
      await vi.advanceTimersByTimeAsync(250)
      expect(save).toHaveBeenCalledOnce()
    } finally {
      vi.useRealTimers()
    }
  })
})
