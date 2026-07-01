import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { getParentPath, schedulePersistSession, usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import type {
  DirectoryEntry,
  ListDirRequest,
  ListDirResponse,
  ListTreeChildrenRequest,
  ListTreeChildrenResponse,
  SaveConfigRequest,
} from '@/lib/types/ipc'

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

function dirAt(path: string, isDir = true): DirectoryEntry {
  const name = path.split('\\').filter(Boolean).at(-1) ?? path
  return {
    ...dir(name, isDir),
    id: path,
    name,
    path,
  }
}

function responder(payload: ListDirRequest): ListDirResponse {
  if (payload.path === 'C:\\fail') {
    throw new Error('Access is denied')
  }
  return { path: payload.path, entries: [dir('Alpha'), dir('Beta', false)] }
}

function treeResponder(payload: ListTreeChildrenRequest): ListTreeChildrenResponse {
  if (payload.path === 'C:\\fail') {
    throw new Error('Access is denied')
  }
  return {
    path: payload.path,
    children: [
      {
        name: 'Alpha',
        path: 'C:\\root\\Alpha',
        hasChildren: false,
      },
    ],
  }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useSelectionStore.getState().reset()
  useTabsStore.getState().reset()
  ipc.override('list_dir', responder)
  ipc.override('list_tree_children', treeResponder)
  ipc.override('set_tab_watch', () => undefined)
  ipc.override('save_session', (payload) => payload.session)
})

describe('panes-store navigation', () => {
  it('navigates, sorts, and records an error', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().panes.left.entries.map((e) => e.name)).toEqual([
      'Alpha',
      'Beta',
    ])

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

  it('restores the previously selected folder as the keyboard focus when returning to its parent', async () => {
    ipc.override('list_dir', (payload) => {
      if (payload.path === 'C:\\root') {
        return {
          path: payload.path,
          entries: [
            dirAt('C:\\root\\Alpha'),
            dirAt('C:\\root\\Beta'),
            dirAt('C:\\root\\Gamma'),
            dirAt('C:\\root\\Delta'),
          ],
        }
      }

      if (payload.path === 'C:\\root\\Gamma') {
        return {
          path: payload.path,
          entries: [dirAt('C:\\root\\Gamma\\Nested A'), dirAt('C:\\root\\Gamma\\Nested B')],
        }
      }

      return responder(payload)
    })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    useSelectionStore
      .getState()
      .setSelection('left', ['C:\\root\\Gamma'], 'C:\\root\\Gamma', 'C:\\root\\Gamma')
    usePanesStore.getState().setFocusedEntry('left', 'C:\\root\\Gamma')

    await usePanesStore.getState().navigatePane('left', 'C:\\root\\Gamma')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('C:\\root\\Gamma\\Nested A')

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('C:\\root\\Gamma')
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

  it('clears the active filter when entering a fresh folder', async () => {
    const listDir = vi.fn(responder)
    ipc.override('list_dir', listDir)
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\root',
          filterDraft: 'report',
          filterApplied: 'report',
          typing: true,
        },
      },
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')

    expect(usePanesStore.getState().panes.left.filterDraft).toBe('')
    expect(usePanesStore.getState().panes.left.filterApplied).toBe('')
    expect(usePanesStore.getState().panes.left.typing).toBe(false)
    expect(listDir).toHaveBeenCalledWith(expect.objectContaining({ filter: '' }))
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

  it('sorts locally without reloading the pane or re-requesting sizes', async () => {
    const listDir = vi.fn(() => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\Alpha'), dirAt('C:\\root\\Beta')],
    }))
    const setTabWatch = vi.fn(() => undefined)
    const requestFolderSizes = vi.fn(() => undefined)
    ipc.override('list_dir', listDir)
    ipc.override('set_tab_watch', setTabWatch)
    ipc.override('request_folder_sizes', requestFolderSizes)
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    listDir.mockClear()
    setTabWatch.mockClear()
    requestFolderSizes.mockClear()

    await usePanesStore.getState().setSort('left', 'name')

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Beta',
      'Alpha',
    ])
    expect(usePanesStore.getState().panes.left.sortDirection).toBe('desc')
    expect(useTabsStore.getState().panes.left.tabs[0]?.sortDirection).toBe('desc')
    expect(listDir).not.toHaveBeenCalled()
    expect(setTabWatch).not.toHaveBeenCalled()
    expect(requestFolderSizes).not.toHaveBeenCalled()
  })

  it('requests item counts only when the items column is visible', async () => {
    const listDir = vi.fn(() => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\Alpha')],
    }))
    ipc.override('list_dir', listDir)

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(listDir).toHaveBeenLastCalledWith(expect.objectContaining({ includeItemCounts: true }))

    useLayoutStore.setState((state) => ({
      columns: state.columns.map((column) =>
        column.key === 'items' ? { ...column, visible: false } : column,
      ),
    }))
    listDir.mockClear()

    await usePanesStore.getState().reloadPane('left')
    expect(listDir).toHaveBeenLastCalledWith(expect.objectContaining({ includeItemCounts: false }))

    useLayoutStore.setState((state) => ({
      columns: state.columns.map((column) =>
        column.key === 'items' ? { ...column, visible: true } : column,
      ),
    }))
  })

  it('sorts by type in both directions locally', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [
        { ...dirAt('C:\\root\\notes.txt', false), typeLabel: 'TXT file' },
        { ...dirAt('C:\\root\\archive.zip', false), typeLabel: 'ZIP file' },
        { ...dirAt('C:\\root\\app.exe', false), typeLabel: 'EXE file' },
      ],
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().setSort('left', 'type')

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'app.exe',
      'notes.txt',
      'archive.zip',
    ])

    await usePanesStore.getState().setSort('left', 'type')
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'archive.zip',
      'notes.txt',
      'app.exe',
    ])
  })

  it('keeps size-sorted panes stable while folder sizes stream in, then re-sorts on demand', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [
        dirAt('C:\\root\\Zulu'),
        dirAt('C:\\root\\Gamma'),
        dirAt('C:\\root\\Beta'),
        dirAt('C:\\root\\Alpha'),
      ],
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().setSort('left', 'size')
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Beta',
      'Gamma',
      'Zulu',
    ])

    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Zulu',
      state: 'ready',
      source: 'everything',
      sizeBytes: 10,
    })
    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Alpha',
      state: 'ready',
      source: 'everything',
      sizeBytes: 50,
    })
    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Beta',
      state: 'calculating',
      source: 'everything',
      sizeBytes: null,
    })

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Beta',
      'Gamma',
      'Zulu',
    ])

    await usePanesStore.getState().setSort('left', 'size')
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Zulu',
      'Alpha',
      'Beta',
      'Gamma',
    ])
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

  it('keeps scroll offsets for history navigation but clears fresh targets', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.getState().setScrollPosition('left', 'C:\\root', 120)
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    usePanesStore.getState().setScrollPosition('left', 'C:\\root\\child', 240)

    await usePanesStore.getState().goBack('left')
    expect(usePanesStore.getState().panes.left.scrollPositions['C:\\root']).toBe(120)

    await usePanesStore.getState().goForward('left')
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().panes.left.scrollPositions['C:\\root']).toBeUndefined()
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
      volumes: [
        {
          mountRoot: 'C:\\',
          label: 'Windows',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
      ],
    })

    const children: Record<string, string> = {
      'C:\\': 'C:\\Users',
      'C:\\Users': 'C:\\Users\\Omega',
    }
    ipc.override(
      'list_tree_children',
      (payload: ListTreeChildrenRequest): ListTreeChildrenResponse => {
        const child = children[payload.path]
        return {
          path: payload.path,
          children: child
            ? [
                {
                  name: child,
                  path: child,
                  hasChildren: Boolean(children[child]),
                },
              ]
            : [],
        }
      },
    )

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
      volumes: [
        {
          mountRoot: 'C:\\',
          label: 'Windows',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
      ],
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
    ipc.override(
      'list_tree_children',
      (payload: ListTreeChildrenRequest): ListTreeChildrenResponse => {
        const child = children[payload.path]
        return {
          path: payload.path,
          children: child
            ? [
                {
                  name: child,
                  path: child,
                  hasChildren: Boolean(children[child]),
                },
              ]
            : [],
        }
      },
    )

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
      volumes: [
        {
          mountRoot: 'C:\\',
          label: 'Windows',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
      ],
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
        {
          mountRoot: 'Z:\\',
          label: 'Share',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: true,
          isRemovable: false,
        },
        {
          mountRoot: 'C:\\',
          label: 'Windows',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
        {
          mountRoot: 'D:\\',
          label: '',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
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
        'C:\\': {
          ...state.treeNodes['C:\\'],
          expanded: true,
          loaded: true,
          children: ['C:\\root'],
        },
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
      {
        mountRoot: 'Y:\\',
        label: 'Archive',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: true,
        isRemovable: false,
      },
      {
        mountRoot: 'C:\\',
        label: 'Windows',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: false,
        isRemovable: false,
      },
    ])

    expect(usePanesStore.getState().treeRoots).toEqual(['C:\\', 'Y:\\'])
    expect(usePanesStore.getState().treeNodes['C:\\'].expanded).toBe(true)
    expect(usePanesStore.getState().treeNodes['C:\\root']).toBeDefined()
    expect(usePanesStore.getState().treeNodes['Y:\\'].name).toBe('Archive (Y:)')
    expect(usePanesStore.getState().treeNodes['D:\\']).toBeUndefined()
  })

  it('requests all direct child directories on open when Everything is available', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      // Rust is the sort authority for list_dir, so the fixture returns
      // entries already in the order the backend would produce.
      entries: [
        dirAt('C:\\root\\Alpha'),
        dirAt('C:\\root\\Bravo'),
        dirAt('C:\\root\\Charlie'),
        dirAt('C:\\root\\notes.txt', false),
      ],
    }))
    usePanesStore.setState((state) => ({
      everythingStatus: { status: 'available', isAvailable: true },
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          visibleStartIndex: 0,
          visibleEndIndex: 0,
        },
      },
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    expect(request).toHaveBeenCalledWith({
      paths: ['C:\\root\\Alpha', 'C:\\root\\Bravo', 'C:\\root\\Charlie'],
    })
  })

  it('does not eager-request sizes on open when Everything is unavailable', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    usePanesStore.setState({ everythingStatus: { status: 'unavailable', isAvailable: false } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    expect(request).not.toHaveBeenCalled()
  })

  it('does not eager-request sizes when auto folder size is disabled, even with Everything available', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\Alpha'), dirAt('C:\\root\\Beta')],
    }))
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })
    useConfigStore.getState().hydrate({ ...useConfigStore.getState(), autoFolderSize: false })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    expect(request).not.toHaveBeenCalled()
    useConfigStore.getState().hydrate({ ...useConfigStore.getState(), autoFolderSize: true })
  })

  it('never requests folder sizes when the visible range changes', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    request.mockClear()

    usePanesStore.getState().setVisibleRange('left', 0, 5)
    usePanesStore.getState().setVisibleRange('left', 5, 10)

    expect(request).not.toHaveBeenCalled()
  })

  it('suppresses duplicate eager requests while pending and after terminal size states', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\Alpha'), dirAt('C:\\root\\Beta')],
    }))
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(request).toHaveBeenCalledWith({ paths: ['C:\\root\\Alpha', 'C:\\root\\Beta'] })

    request.mockClear()
    await usePanesStore.getState().reloadPane('left')
    expect(request).not.toHaveBeenCalled()

    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Alpha',
      state: 'ready',
      source: 'everything',
      sizeBytes: 12,
    })
    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Beta',
      state: 'na',
      source: 'everything',
      sizeBytes: null,
    })

    request.mockClear()
    await usePanesStore.getState().reloadPane('left')
    expect(request).not.toHaveBeenCalled()
  })

  it('does not retry eager size requests after an error terminal state until explicit refresh', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\Alpha')],
    }))
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(request).toHaveBeenCalledWith({ paths: ['C:\\root\\Alpha'] })

    usePanesStore.getState().applySizeState({
      path: 'C:\\root\\Alpha',
      state: 'error',
      source: 'everything',
      sizeBytes: null,
    })

    request.mockClear()
    await usePanesStore.getState().reloadPane('left')
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

describe('panes-store icons', () => {
  it('patches an entry with the resolved icon and clears the pending request', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\installer.exe', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.setState({
      pendingIconRequests: { 'C:\\root\\installer.exe': true },
    })

    usePanesStore.getState().applyIconState({
      path: 'C:\\root\\installer.exe',
      iconDataUrl: 'data:image/png;base64,abc',
    })

    const entry = usePanesStore
      .getState()
      .panes.left.entries.find((item) => item.path === 'C:\\root\\installer.exe')
    expect(entry?.iconDataUrl).toBe('data:image/png;base64,abc')
    expect(usePanesStore.getState().pendingIconRequests['C:\\root\\installer.exe']).toBeUndefined()
  })

  it('requests icons only for visible, iconless files and dedupes pending requests', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_icons', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [
        dirAt('C:\\root\\Alpha'),
        dirAt('C:\\root\\installer.exe', false),
        dirAt('C:\\root\\readme.txt', false),
      ],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    await usePanesStore
      .getState()
      .requestVisibleIcons('left', [
        'C:\\root\\Alpha',
        'C:\\root\\installer.exe',
        'C:\\root\\readme.txt',
      ])

    expect(request).toHaveBeenCalledWith({
      paths: ['C:\\root\\installer.exe', 'C:\\root\\readme.txt'],
    })

    request.mockClear()
    await usePanesStore
      .getState()
      .requestVisibleIcons('left', ['C:\\root\\installer.exe', 'C:\\root\\readme.txt'])
    expect(request).not.toHaveBeenCalled()
  })

  it('does not re-request an icon once resolved', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_icons', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\installer.exe', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.getState().applyIconState({
      path: 'C:\\root\\installer.exe',
      iconDataUrl: 'data:image/png;base64,abc',
    })

    await usePanesStore.getState().requestVisibleIcons('left', ['C:\\root\\installer.exe'])
    expect(request).not.toHaveBeenCalled()
  })

  it('never re-requests a file that resolves to no icon, even repeatedly (regression: infinite request loop)', async () => {
    // Most files never get a native icon (non-Windows, or a Windows
    // extension outside the allowlist), so `iconDataUrl` stays null forever.
    // Treating "still null" as "still needs a request" caused an endless
    // request_icons loop for entire folders like Downloads.
    const request = vi.fn(() => undefined)
    ipc.override('request_icons', request)
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\readme.txt', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    await usePanesStore.getState().requestVisibleIcons('left', ['C:\\root\\readme.txt'])
    expect(request).toHaveBeenCalledTimes(1)

    usePanesStore.getState().applyIconState({
      path: 'C:\\root\\readme.txt',
      iconDataUrl: null,
    })
    expect(
      usePanesStore.getState().panes.left.entries.find((entry) => entry.path === 'C:\\root\\readme.txt')
        ?.iconDataUrl,
    ).toBeNull()

    // Simulate the effect re-firing (e.g. because the entries array reference
    // changed) any number of times: the path must stay resolved.
    for (let i = 0; i < 5; i += 1) {
      await usePanesStore.getState().requestVisibleIcons('left', ['C:\\root\\readme.txt'])
    }
    expect(request).toHaveBeenCalledTimes(1)
  })
})
