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

  it('records history and steps back and forward', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    expect(usePanesStore.getState().panes.left.history).toEqual(['C:\\root', 'C:\\root\\child'])
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(1)

    await usePanesStore.getState().goBack('left')
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root')
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(0)

    await usePanesStore.getState().goForward('left')
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root\\child')
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(1)
  })

  it('treats back at the start and forward at the end as no-ops', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    await usePanesStore.getState().goBack('left')
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root')
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(0)

    await usePanesStore.getState().goForward('left')
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root')
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(0)
  })

  it('truncates forward history when navigating after going back', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    await usePanesStore.getState().goBack('left')
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\other')

    expect(usePanesStore.getState().panes.left.history).toEqual(['C:\\root', 'C:\\root\\other'])
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(1)
  })

  it('does not grow history when navigating to the current path', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().panes.left.history).toEqual(['C:\\root'])
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(0)
  })

  it('resets pane history when a new tab is opened', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    await usePanesStore.getState().openTabFromPath('left', 'C:\\root\\Alpha')

    expect(usePanesStore.getState().panes.left.history).toEqual(['C:\\root\\Alpha'])
    expect(usePanesStore.getState().panes.left.historyIndex).toBe(0)

    await usePanesStore.getState().goBack('left')
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root\\Alpha')
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

  it('reveals the active path by expanding its ancestor chain down to the leaf', async () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: 'C:\\', rightPath: 'C:\\' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [{ mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false }],
    })

    const children: Record<string, string> = {
      'C:\\': 'C:\\Users',
      'C:\\Users': 'C:\\Users\\Omega',
    }
    ipc.override('list_dir', (payload: ListDirRequest): ListDirResponse => {
      const child = children[payload.path]
      return {
        path: payload.path,
        entries: child ? [{ ...dir('child'), id: child, name: child, path: child }] : [],
      }
    })

    await usePanesStore.getState().revealPath('C:\\Users\\Omega')

    const nodes = usePanesStore.getState().treeNodes
    expect(nodes['C:\\'].expanded).toBe(true)
    expect(nodes['C:\\Users'].expanded).toBe(true)
    // The leaf is rendered but not force-expanded.
    expect(nodes['C:\\Users\\Omega']).toBeDefined()
  })

  it('collapses unrelated branches when revealing a different path', async () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: 'C:\\', rightPath: 'C:\\' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [{ mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false }],
    })

    // Seed an expanded, unrelated branch that should collapse on reveal.
    usePanesStore.setState((state) => ({
      treeNodes: {
        ...state.treeNodes,
        'C:\\Other': {
          id: 'C:\\Other',
          name: 'Other',
          path: 'C:\\Other',
          parentPath: 'C:\\',
          children: [],
          expanded: true,
          loaded: true,
        },
      },
    }))

    const children: Record<string, string> = {
      'C:\\': 'C:\\Users',
      'C:\\Users': 'C:\\Users\\Omega',
    }
    ipc.override('list_dir', (payload: ListDirRequest): ListDirResponse => {
      const child = children[payload.path]
      return {
        path: payload.path,
        entries: child ? [{ ...dir('child'), id: child, name: child, path: child }] : [],
      }
    })

    await usePanesStore.getState().revealPath('C:\\Users\\Omega')

    const nodes = usePanesStore.getState().treeNodes
    expect(nodes['C:\\Users'].expanded).toBe(true)
    expect(nodes['C:\\Other'].expanded).toBe(false)
  })

  it('ignores reveal for empty or out-of-volume paths', async () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: 'C:\\', rightPath: 'C:\\' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [{ mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false }],
    })

    await usePanesStore.getState().revealPath(null)
    await usePanesStore.getState().revealPath('D:\\outside')
    expect(usePanesStore.getState().treeNodes['D:\\outside']).toBeUndefined()
  })

  it('sorts tree roots by drive letter, formats labels, and keeps tree state in sync with volume changes', () => {
    usePanesStore.getState().initialize({
      session: {
        activePane: 'left',
        leftPath: 'C:\\root',
        rightPath: 'D:\\work',
      },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        {
          mountRoot: '\\\\nas\\media',
          label: 'Media Share',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: true,
          isRemovable: false,
        },
        { mountRoot: 'Z:\\', label: 'Share', totalBytes: 1, freeBytes: 1, isNetwork: true, isRemovable: false },
        { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
        { mountRoot: 'D:\\', label: '', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
      ],
    })

    expect(usePanesStore.getState().treeRoots).toEqual(['\\\\nas\\media', 'C:\\', 'D:\\', 'Z:\\'])
    expect(usePanesStore.getState().treeNodes['C:\\'].name).toBe('Windows (C:)')
    expect(usePanesStore.getState().treeNodes['D:\\'].name).toBe('D:')
    expect(usePanesStore.getState().treeNodes['Z:\\'].name).toBe('Share (Z:)')
    expect(usePanesStore.getState().treeNodes['\\\\nas\\media'].name).toBe('Media Share')

    usePanesStore.setState((state) => ({
      treeNodes: {
        ...state.treeNodes,
        'C:\\': { ...state.treeNodes['C:\\'], expanded: true, loaded: true, children: ['C:\\root'] },
        'C:\\root': {
          id: 'C:\\root',
          name: 'root',
          path: 'C:\\root',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: false,
        },
      },
    }))

    usePanesStore.getState().setVolumes([
      { mountRoot: 'Y:\\', label: 'Archive', totalBytes: 1, freeBytes: 1, isNetwork: true, isRemovable: false },
      { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
    ])

    expect(usePanesStore.getState().treeRoots).toEqual(['C:\\', 'Y:\\'])
    expect(usePanesStore.getState().treeNodes['C:\\'].expanded).toBe(true)
    expect(usePanesStore.getState().treeNodes['C:\\root']).toBeDefined()
    expect(usePanesStore.getState().treeNodes['Y:\\'].name).toBe('Archive (Y:)')
    expect(usePanesStore.getState().treeNodes['D:\\']).toBeUndefined()
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
    expect(getParentPath('\\\\nas')).toBeNull()
    expect(getParentPath('\\\\nas\\media')).toBeNull()
    expect(getParentPath('\\\\nas\\media\\shows')).toBe('\\\\nas\\media')
    expect(getParentPath('\\\\?\\UNC\\nas\\media')).toBeNull()
    expect(getParentPath('\\\\?\\UNC\\nas\\media\\shows')).toBe('\\\\nas\\media')
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
