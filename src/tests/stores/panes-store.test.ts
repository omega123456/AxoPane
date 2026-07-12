import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { getParentPath, schedulePersistSession, usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
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
        expandability: 'empty',
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
    ipc.override('list_dir', (payload) => ({
      path: payload.path,
      entries:
        payload.path === 'C:\\root'
          ? [dirAt('C:\\root\\child'), dirAt('C:\\root\\sibling')]
          : [dirAt('C:\\root\\child\\nested')],
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    await usePanesStore.getState().goUp('left')

    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('C:\\root\\child')
    expect(useSelectionStore.getState().selections.left).toEqual({
      selectedIds: ['C:\\root\\child'],
      anchorId: 'C:\\root\\child',
      focusedId: 'C:\\root\\child',
    })
  })

  it('selects the folder being left when going up a POSIX path', async () => {
    ipc.override('list_dir', (payload) => ({
      path: payload.path,
      entries:
        payload.path === '/Users/example'
          ? [dirAt('/Users/example/Documents'), dirAt('/Users/example/Downloads')]
          : [dirAt('/Users/example/Documents/report.txt', false)],
    }))

    await usePanesStore.getState().navigatePane('left', '/Users/example/Documents')
    await usePanesStore.getState().goUp('left')

    expect(usePanesStore.getState().panes.left.path).toBe('/Users/example')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('/Users/example/Documents')
    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual([
      '/Users/example/Documents',
    ])
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

  it('requests sizes for every folder in the pane, skipping files and already-pending paths', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.setState((state) => ({
      pendingSizeRequests: { ...state.pendingSizeRequests, 'C:\\root\\Alpha': true },
    }))

    await usePanesStore.getState().calculateAllFolderSizes('left')

    expect(request).not.toHaveBeenCalled()

    usePanesStore.setState((state) => {
      const rest = { ...state.pendingSizeRequests }
      delete rest['C:\\root\\Alpha']
      return { pendingSizeRequests: rest }
    })

    await usePanesStore.getState().calculateAllFolderSizes('left')

    expect(request).toHaveBeenCalledOnce()
    expect(request).toHaveBeenCalledWith({ paths: ['C:\\root\\Alpha'] })
  })

  it('clears pending size requests when calculating all folder sizes fails', async () => {
    ipc.override('request_folder_sizes', () => {
      throw new Error('request failed')
    })
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    await expect(usePanesStore.getState().calculateAllFolderSizes('left')).rejects.toThrow(
      'request failed',
    )
    expect(usePanesStore.getState().pendingSizeRequests['C:\\root\\Alpha']).toBeFalsy()
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

  it('requests normal listings and watches without synchronous item counts', async () => {
    const listDir = vi.fn(() => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\Alpha'), dirAt('C:\\root\\Beta', false)],
    }))
    const setTabWatch = vi.fn(() => undefined)
    ipc.override('list_dir', listDir)
    ipc.override('set_tab_watch', setTabWatch)

    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    expect(listDir).toHaveBeenLastCalledWith(expect.objectContaining({ includeItemCounts: false }))
    expect(setTabWatch).toHaveBeenLastCalledWith(
      expect.objectContaining({
        target: expect.objectContaining({ includeItemCounts: false }),
        seedReference: expect.objectContaining({
          path: 'C:\\root',
          requestId: expect.any(Number),
          tabId: expect.any(String),
        }),
      }),
    )
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

    usePanesStore.getState().applySizeStates([
      {
        path: 'C:\\root\\Zulu',
        state: 'ready',
        source: 'everything',
        sizeBytes: 10,
      },
      {
        path: 'C:\\root\\Alpha',
        state: 'ready',
        source: 'everything',
        sizeBytes: 50,
      },
      {
        path: 'C:\\root\\Beta',
        state: 'calculating',
        source: 'everything',
        sizeBytes: null,
      },
    ])

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
    usePanesStore.getState().applySizeStates([
      {
        path: 'C:\\root\\Alpha',
        state: 'ready',
        source: 'everything',
        sizeBytes: 5,
      },
    ])
    await usePanesStore.getState().refreshEverything('left')
    expect(usePanesStore.getState().sizeStates['C:\\root\\Alpha']).toBeUndefined()
  })

  it('applySizeStates is a no-op for an empty batch', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    const sizeStatesBefore = usePanesStore.getState().sizeStates

    usePanesStore.getState().applySizeStates([])

    expect(usePanesStore.getState().sizeStates).toBe(sizeStatesBefore)
  })

  it('opens, switches (single enumeration), and closes tabs', async () => {
    const listDir = vi.fn((payload: { path: string }) => ({
      path: payload.path,
      entries:
        payload.path === 'C:\\root' ? [dirAt('C:\\root\\Alpha'), dirAt('C:\\root\\Beta')] : [],
    }))
    ipc.override('list_dir', listDir)

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().openTabFromPath('left', 'C:\\root\\Alpha')
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(2)
    expect(usePanesStore.getState().panes.left.path).toBe('C:\\root\\Alpha')

    const firstTabId = useTabsStore.getState().panes.left.tabs[0].id
    listDir.mockClear()
    await usePanesStore.getState().switchTab('left', firstTabId)
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
    expect(listDir).toHaveBeenCalledTimes(1)

    // switching to the already-active tab is a no-op activation
    await usePanesStore.getState().switchTab('left', firstTabId)
    expect(usePanesStore.getState().activePaneId).toBe('left')

    const secondTabId = useTabsStore.getState().panes.left.tabs[1].id
    await usePanesStore.getState().closeTab('left', secondTabId)
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
  })

  it('bumps the focus request for the pane a new tab is opened into', async () => {
    expect(usePanesStore.getState().focusRequestId).toBe(0)
    expect(usePanesStore.getState().focusRequestPaneId).toBeNull()

    await usePanesStore.getState().openTabFromPath('right', 'C:\\root')
    expect(usePanesStore.getState().focusRequestId).toBe(1)
    expect(usePanesStore.getState().focusRequestPaneId).toBe('right')
    expect(usePanesStore.getState().activePaneId).toBe('right')

    await usePanesStore.getState().openTabFromPath('left', 'C:\\root\\Alpha')
    expect(usePanesStore.getState().focusRequestId).toBe(2)
    expect(usePanesStore.getState().focusRequestPaneId).toBe('left')
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

  it('binary-inserts a new folder into its sorted slot without re-mapping untouched rows', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dir('Alpha'), dir('Charlie'), dir('mango.txt', false), dir('zebra.txt', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    const before = usePanesStore.getState().panes.left.entries
    const alphaRef = before[0]

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [{ path: 'C:\\root\\Bravo', entry: dir('Bravo') }],
      removed: [],
    })

    const after = usePanesStore.getState().panes.left.entries
    expect(after.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Bravo',
      'Charlie',
      'mango.txt',
      'zebra.txt',
    ])
    // Untouched rows keep their identity (no full-listing re-map/re-sort).
    expect(after[0]).toBe(alphaRef)
  })

  it('patches loaded tree children from accepted directory patches without refetching', () => {
    const listTreeChildren = vi.fn(treeResponder)
    ipc.override('list_tree_children', listTreeChildren)
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\root',
          entries: [dir('Alpha'), dir('Zulu')],
          focusedEntryId: 'Alpha',
        },
      },
      treeNodes: {
        'C:\\root': {
          id: 'C:\\root',
          name: 'root',
          path: 'C:\\root',
          parentPath: 'C:\\',
          children: ['C:\\root\\Alpha', 'C:\\root\\Zulu'],
          expanded: true,
          loaded: true,
        },
        'C:\\root\\Alpha': {
          id: 'C:\\root\\Alpha',
          name: 'Alpha',
          path: 'C:\\root\\Alpha',
          parentPath: 'C:\\root',
          children: [],
          expanded: false,
          loaded: false,
        },
        'C:\\root\\Zulu': {
          id: 'C:\\root\\Zulu',
          name: 'Zulu',
          path: 'C:\\root\\Zulu',
          parentPath: 'C:\\root',
          children: [],
          expanded: false,
          loaded: false,
        },
      },
    }))

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [{ path: 'C:\\root\\Bravo', entry: dir('Bravo') }],
      removed: [],
    })

    expect(usePanesStore.getState().treeNodes['C:\\root'].children).toEqual([
      'C:\\root\\Alpha',
      'C:\\root\\Bravo',
      'C:\\root\\Zulu',
    ])
    expect(usePanesStore.getState().treeNodes['C:\\root\\Bravo']).toMatchObject({
      name: 'Bravo',
      parentPath: 'C:\\root',
      loaded: false,
    })
    expect(listTreeChildren).not.toHaveBeenCalled()
  })

  it('removes deleted tree children and their cached descendants from directory patches', () => {
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\root',
          entries: [dir('Alpha')],
          focusedEntryId: 'Alpha',
        },
      },
      treeNodes: {
        'C:\\root': {
          id: 'C:\\root',
          name: 'root',
          path: 'C:\\root',
          parentPath: 'C:\\',
          children: ['C:\\root\\Alpha'],
          expanded: true,
          loaded: true,
        },
        'C:\\root\\Alpha': {
          id: 'C:\\root\\Alpha',
          name: 'Alpha',
          path: 'C:\\root\\Alpha',
          parentPath: 'C:\\root',
          children: ['C:\\root\\Alpha\\Nested'],
          expanded: true,
          loaded: true,
        },
        'C:\\root\\Alpha\\Nested': {
          id: 'C:\\root\\Alpha\\Nested',
          name: 'Nested',
          path: 'C:\\root\\Alpha\\Nested',
          parentPath: 'C:\\root\\Alpha',
          children: [],
          expanded: false,
          loaded: false,
        },
      },
    }))

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [],
      removed: ['C:\\root\\Alpha'],
    })

    expect(usePanesStore.getState().treeNodes['C:\\root'].children).toEqual([])
    expect(usePanesStore.getState().treeNodes['C:\\root\\Alpha']).toBeUndefined()
    expect(usePanesStore.getState().treeNodes['C:\\root\\Alpha\\Nested']).toBeUndefined()
  })

  it('keeps the tree cache untouched for file-only directory patches', () => {
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\root',
          entries: [],
        },
      },
      treeNodes: {
        'C:\\root': {
          id: 'C:\\root',
          name: 'root',
          path: 'C:\\root',
          parentPath: 'C:\\',
          children: [],
          expanded: true,
          loaded: true,
        },
      },
    }))
    const before = usePanesStore.getState().treeNodes

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [{ path: 'C:\\root\\note.txt', entry: dir('note.txt', false) }],
      removed: [],
    })

    expect(usePanesStore.getState().treeNodes).toBe(before)
  })

  it('inserts a new file after the folders and among the files', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dir('Alpha'), dir('mango.txt', false), dir('zebra.txt', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [{ path: 'C:\\root\\nova.txt', entry: dir('nova.txt', false) }],
      removed: [],
    })

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'mango.txt',
      'nova.txt',
      'zebra.txt',
    ])
  })

  it('applies a rename (remove old + insert new) at the renamed sorted position', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dir('Alpha'), dir('Charlie'), dir('mango.txt', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [{ path: 'C:\\root\\Delta', entry: dir('Delta') }],
      removed: ['C:\\root\\Charlie'],
    })

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Delta',
      'mango.txt',
    ])
  })

  it('drops a changed entry that no longer matches the pane (now hidden)', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dir('Alpha'), dir('Bravo'), dir('Charlie')],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [{ path: 'C:\\root\\Bravo', entry: { ...dir('Bravo'), isHidden: true } }],
      removed: [],
    })

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Charlie',
    ])
  })

  it('reorders a same-path modify under a non-name sort key (modified)', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [
        { ...dir('a.txt', false), modifiedAt: '2024-01-03T00:00:00Z' },
        { ...dir('b.txt', false), modifiedAt: '2024-01-02T00:00:00Z' },
        { ...dir('c.txt', false), modifiedAt: '2024-01-01T00:00:00Z' },
      ],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    // Sort by modified: first click yields descending (newest first).
    await usePanesStore.getState().setSort('left', 'modified')
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'a.txt',
      'b.txt',
      'c.txt',
    ])

    // c.txt becomes the newest -> it must move to the front.
    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [
        {
          path: 'C:\\root\\c.txt',
          entry: { ...dir('c.txt', false), modifiedAt: '2024-01-09T00:00:00Z' },
        },
      ],
      removed: [],
    })

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'c.txt',
      'a.txt',
      'b.txt',
    ])
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

  it('selects the folder being left when history navigation returns to its parent', async () => {
    ipc.override('list_dir', (payload) => ({
      path: payload.path,
      entries:
        payload.path === 'C:\\root'
          ? [dirAt('C:\\root\\child'), dirAt('C:\\root\\sibling')]
          : [dirAt('C:\\root\\child\\nested')],
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    useSelectionStore.getState().clearSelectionForPane('left')

    await usePanesStore.getState().goBack('left')

    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('C:\\root\\child')
    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual(['C:\\root\\child'])
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

  it('corrects a stale path-derived node name once the real parent listing arrives', async () => {
    // Simulates a node created before its parent was ever listed (e.g. a pane
    // navigating straight to it), which previously stuck with its raw path as
    // the display name forever since the merge below never refreshed it.
    usePanesStore.setState((state) => ({
      treeNodes: {
        ...state.treeNodes,
        'C:\\Users\\Omega\\Downloads': {
          id: 'C:\\Users\\Omega\\Downloads',
          name: 'C:\\Users\\Omega\\Downloads',
          path: 'C:\\Users\\Omega\\Downloads',
          parentPath: null,
          children: [],
          expanded: false,
          loaded: false,
        },
      },
    }))

    ipc.override(
      'list_tree_children',
      (): ListTreeChildrenResponse => ({
        path: 'C:\\Users\\Omega',
        children: [{ name: 'Downloads', path: 'C:\\Users\\Omega\\Downloads', expandability: 'nonEmpty' }],
      }),
    )
    await usePanesStore.getState().ensureTreeChildren('C:\\Users\\Omega')

    expect(usePanesStore.getState().treeNodes['C:\\Users\\Omega\\Downloads']).toMatchObject({
      name: 'Downloads',
      parentPath: 'C:\\Users\\Omega',
    })
  })

  it('names a newly expanded leaf from its path when its parent has not been listed yet', async () => {
    await usePanesStore.getState().toggleTreeNode('C:\\Users\\Omega\\Downloads')
    expect(usePanesStore.getState().treeNodes['C:\\Users\\Omega\\Downloads'].name).toBe('Downloads')
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
                  expandability: children[child] ? 'nonEmpty' : 'empty',
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

  it('reveals macOS network mount paths from the deepest mounted volume root', async () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: '/', rightPath: '/' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        {
          mountRoot: '/',
          label: 'Macintosh HD',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
        {
          mountRoot: '/Volumes/team',
          label: 'Team Share',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: true,
          isRemovable: false,
        },
      ],
    })

    const listedPaths: string[] = []
    ipc.override(
      'list_tree_children',
      (payload: ListTreeChildrenRequest): ListTreeChildrenResponse => {
        listedPaths.push(payload.path)
        return {
          path: payload.path,
          children:
            payload.path === '/Volumes/team'
              ? [{ name: 'Project', path: '/Volumes/team/Project', expandability: 'empty' }]
              : [],
        }
      },
    )

    await usePanesStore.getState().revealPath('/Volumes/team/Project')

    const nodes = usePanesStore.getState().treeNodes
    expect(listedPaths).toEqual(['/Volumes/team'])
    expect(nodes['/'].expanded).toBe(false)
    expect(nodes['/Volumes']).toBeUndefined()
    expect(nodes['/Volumes/team'].parentPath).toBeNull()
    expect(nodes['/Volumes/team'].expanded).toBe(true)
    expect(nodes['/Volumes/team/Project']).toMatchObject({
      parentPath: '/Volumes/team',
      expanded: false,
    })
  })

  it('keeps mounted volume roots detached when listing their /Volumes parent', async () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: '/', rightPath: '/' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        {
          mountRoot: '/',
          label: 'Macintosh HD',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
        {
          mountRoot: '/Volumes/team',
          label: 'Team Share',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: true,
          isRemovable: false,
        },
      ],
    })

    ipc.override(
      'list_tree_children',
      (payload: ListTreeChildrenRequest): ListTreeChildrenResponse => ({
        path: payload.path,
        children:
          payload.path === '/Volumes'
            ? [
                { name: 'team', path: '/Volumes/team', expandability: 'nonEmpty' },
                { name: 'local', path: '/Volumes/local', expandability: 'empty' },
              ]
            : [],
      }),
    )

    await usePanesStore.getState().ensureTreeChildren('/Volumes')

    const nodes = usePanesStore.getState().treeNodes
    expect(nodes['/Volumes'].children).toEqual(['/Volumes/team', '/Volumes/local'])
    expect(nodes['/Volumes/team']).toMatchObject({
      name: 'Team Share',
      parentPath: null,
    })
    expect(nodes['/Volumes/local']).toMatchObject({
      name: 'local',
      parentPath: '/Volumes',
    })
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
                  expandability: children[child] ? 'nonEmpty' : 'empty',
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
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

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

    usePanesStore.getState().applySizeStates([
      {
        path: 'C:\\root\\Alpha',
        state: 'ready',
        source: 'everything',
        sizeBytes: 12,
      },
      {
        path: 'C:\\root\\Beta',
        state: 'na',
        source: 'everything',
        sizeBytes: null,
      },
    ])

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

    usePanesStore.getState().applySizeStates([
      {
        path: 'C:\\root\\Alpha',
        state: 'error',
        source: 'everything',
        sizeBytes: null,
      },
    ])

    request.mockClear()
    await usePanesStore.getState().reloadPane('left')
    expect(request).not.toHaveBeenCalled()
  })

  it('gates eager sizing on the per-pane 500-folder limit', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_folder_sizes', request)
    ipc.override('list_dir', (payload) => ({
      path: payload.path,
      entries: Array.from({ length: payload.path === 'C:\\many' ? 501 : 500 }, (_, index) =>
        dirAt(`${payload.path}\\dir-${index}`),
      ),
    }))
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    // Exactly 500 folders is still eagerly sized.
    await usePanesStore.getState().navigatePane('left', 'C:\\few')
    expect(request).toHaveBeenCalledTimes(1)
    expect(request).toHaveBeenCalledWith({
      paths: Array.from({ length: 500 }, (_, index) => `C:\\few\\dir-${index}`),
    })

    // 501 folders crosses the limit, so auto-sizing is suppressed for the pane.
    request.mockClear()
    await usePanesStore.getState().navigatePane('left', 'C:\\many')
    expect(request).not.toHaveBeenCalled()
  })

  it('cancels in-flight folder-size jobs for the folder it navigates away from', async () => {
    const cancel = vi.fn((payload: { paths: string[] }) => ({ cancelled: payload.paths.length }))
    ipc.override('cancel_sizes', cancel)
    ipc.override('request_folder_sizes', () => undefined)
    ipc.override('list_dir', (payload) =>
      payload.path === 'C:\\root'
        ? { path: 'C:\\root', entries: [dirAt('C:\\root\\Alpha'), dirAt('C:\\root\\Beta')] }
        : { path: payload.path, entries: [] },
    )
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().pendingSizeRequests['C:\\root\\Alpha']).toBe(true)

    // Alpha is actively calculating; cancellation should drop its transient state.
    usePanesStore
      .getState()
      .applySizeStates([
        { path: 'C:\\root\\Alpha', state: 'calculating', source: 'everything', sizeBytes: null },
      ])

    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')

    expect(cancel).toHaveBeenCalledWith({ paths: ['C:\\root\\Alpha', 'C:\\root\\Beta'] })
    expect(usePanesStore.getState().pendingSizeRequests['C:\\root\\Alpha']).toBeFalsy()
    expect(usePanesStore.getState().pendingSizeRequests['C:\\root\\Beta']).toBeFalsy()
    expect(usePanesStore.getState().sizeStates['C:\\root\\Alpha']).toBeUndefined()
  })

  it('leaves size jobs running for a folder still shown in the other pane', async () => {
    const cancel = vi.fn((payload: { paths: string[] }) => ({ cancelled: payload.paths.length }))
    ipc.override('cancel_sizes', cancel)
    ipc.override('request_folder_sizes', () => undefined)
    ipc.override('list_dir', (payload) =>
      payload.path === 'C:\\root'
        ? { path: 'C:\\root', entries: [dirAt('C:\\root\\Alpha')] }
        : { path: payload.path, entries: [] },
    )
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    // Both panes land on C:\root, so Alpha is shared.
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().navigatePane('right', 'C:\\root')

    // Leaving C:\root in the left pane must not cancel Alpha — the right pane
    // still shows it.
    await usePanesStore.getState().navigatePane('left', 'C:\\root\\child')
    expect(cancel).not.toHaveBeenCalled()
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

    usePanesStore.getState().applyIconStates([
      {
        path: 'C:\\root\\installer.exe',
        iconDataUrl: 'data:image/png;base64,abc',
      },
    ])

    const entry = usePanesStore
      .getState()
      .panes.left.entries.find((item) => item.path === 'C:\\root\\installer.exe')
    expect(entry?.iconDataUrl).toBe('data:image/png;base64,abc')
    expect(usePanesStore.getState().pendingIconRequests['C:\\root\\installer.exe']).toBeUndefined()
  })

  it('hydrates a later pane from the resolved icon cache for the same path', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_icons', request)
    ipc.override('list_dir', (payload) => ({
      path: payload.path,
      entries: [dirAt(`${payload.path}\\installer.exe`, false)],
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.getState().applyIconStates([
      {
        path: 'C:\\root\\installer.exe',
        iconDataUrl: 'data:image/png;base64,abc',
      },
    ])

    await usePanesStore.getState().navigatePane('right', 'C:\\root')

    const rightEntry = usePanesStore
      .getState()
      .panes.right.entries.find((item) => item.path === 'C:\\root\\installer.exe')
    expect(rightEntry?.iconDataUrl).toBe('data:image/png;base64,abc')

    await usePanesStore.getState().requestVisibleIcons('right', ['C:\\root\\installer.exe'])
    expect(request).not.toHaveBeenCalled()
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

    usePanesStore.getState().applyIconStates([
      {
        path: 'C:\\root\\installer.exe',
        iconDataUrl: 'data:image/png;base64,abc',
      },
    ])

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

    usePanesStore.getState().applyIconStates([
      {
        path: 'C:\\root\\readme.txt',
        iconDataUrl: null,
      },
    ])
    expect(
      usePanesStore
        .getState()
        .panes.left.entries.find((entry) => entry.path === 'C:\\root\\readme.txt')?.iconDataUrl,
    ).toBeNull()

    // Simulate the effect re-firing (e.g. because the entries array reference
    // changed) any number of times: the path must stay resolved.
    for (let i = 0; i < 5; i += 1) {
      await usePanesStore.getState().requestVisibleIcons('left', ['C:\\root\\readme.txt'])
    }
    expect(request).toHaveBeenCalledTimes(1)
  })

  it('applyIconStates is a no-op for an empty batch', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\a.exe', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    const panesBefore = usePanesStore.getState().panes
    const resolvedIconPathsBefore = usePanesStore.getState().resolvedIconPaths
    usePanesStore.getState().applyIconStates([])

    expect(usePanesStore.getState().panes).toBe(panesBefore)
    expect(usePanesStore.getState().resolvedIconPaths).toBe(resolvedIconPathsBefore)
  })

  it('applyIconStates batches an entire buffered burst into one panes update, patching every matched entry', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\a.exe', false), dirAt('C:\\root\\b.exe', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.getState().applyIconStates([
      { path: 'C:\\root\\a.exe', iconDataUrl: 'data:image/png;base64,aaa' },
      { path: 'C:\\root\\b.exe', iconDataUrl: 'data:image/png;base64,bbb' },
    ])

    const entries = usePanesStore.getState().panes.left.entries
    expect(entries.find((entry) => entry.path === 'C:\\root\\a.exe')?.iconDataUrl).toBe(
      'data:image/png;base64,aaa',
    )
    expect(entries.find((entry) => entry.path === 'C:\\root\\b.exe')?.iconDataUrl).toBe(
      'data:image/png;base64,bbb',
    )
  })

  it('applyIconStates skips a matched entry whose icon is already identical, still updating an unrelated matched entry', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\a.exe', false), dirAt('C:\\root\\b.exe', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore
      .getState()
      .applyIconStates([{ path: 'C:\\root\\a.exe', iconDataUrl: 'data:image/png;base64,aaa' }])
    const entriesAfterFirst = usePanesStore.getState().panes.left.entries

    // Re-sending the identical icon for `a.exe` alongside a genuinely new
    // icon for `b.exe` must still patch `b.exe` while leaving `a.exe`'s
    // entry object untouched (exercises the per-event no-op continue).
    usePanesStore.getState().applyIconStates([
      { path: 'C:\\root\\a.exe', iconDataUrl: 'data:image/png;base64,aaa' },
      { path: 'C:\\root\\b.exe', iconDataUrl: 'data:image/png;base64,bbb' },
    ])

    const entriesAfterSecond = usePanesStore.getState().panes.left.entries
    expect(entriesAfterSecond.find((entry) => entry.path === 'C:\\root\\a.exe')).toBe(
      entriesAfterFirst.find((entry) => entry.path === 'C:\\root\\a.exe'),
    )
    expect(entriesAfterSecond.find((entry) => entry.path === 'C:\\root\\b.exe')?.iconDataUrl).toBe(
      'data:image/png;base64,bbb',
    )
  })

  it('applyIconStates leaves the panes reference untouched when no buffered path matches any entry (FR7)', async () => {
    ipc.override('list_dir', () => ({
      path: 'C:\\root',
      entries: [dirAt('C:\\root\\a.exe', false)],
    }))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    const panesBefore = usePanesStore.getState().panes

    usePanesStore
      .getState()
      .applyIconStates([
        { path: 'C:\\root\\does-not-exist.exe', iconDataUrl: 'data:image/png;base64,zzz' },
      ])

    expect(usePanesStore.getState().panes).toBe(panesBefore)
    // Still recorded in the resolved-icon cache so this path is never re-requested.
    expect(usePanesStore.getState().resolvedIconPaths['C:\\root\\does-not-exist.exe']).toBe(
      'data:image/png;base64,zzz',
    )
  })
})

describe('panes-store v2 directory sessions', () => {
  // Live navigation now drives `begin_directory_session` /
  // `get_directory_session_range` (`ListingSession`/`PaneEntryCollection`)
  // instead of `start_list_dir` + `dir://list-chunk`. These tests replace the
  // old "panes-store streamed listings" block; `applyListChunk` itself (the
  // v1 streamed-chunk-application API) is still exercised directly by the
  // dedicated tests at the end of this block, since it remains a public store
  // action even though live navigation no longer reaches it.
  function beginResponse(overrides: {
    entries: DirectoryEntry[]
    sessionId: number
    path?: string
  }) {
    return (payload: { paneId: string; tabId: string; path: string }) => ({
      paneId: payload.paneId,
      tabId: payload.tabId,
      path: overrides.path ?? payload.path,
      baseline: {
        sessionId: overrides.sessionId,
        navigationRevision: overrides.sessionId,
        watchRevision: 0,
        viewRevision: 0,
      },
      totalRows: overrides.entries.length,
      pageSize: 500,
      firstPage: { pageIndex: 0, entries: overrides.entries },
    })
  }

  it('shows entries once the session begins', async () => {
    ipc.override(
      'begin_directory_session',
      beginResponse({ entries: [dir('Alpha'), dir('Bravo'), dir('Charlie')], sessionId: 7 }),
    )

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    expect(usePanesStore.getState().panes.left.loading).toBe(false)
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Bravo',
      'Charlie',
    ])
  })

  it('stitches multiple v2 range pages into the pane in page order', async () => {
    const pageOneEntries = Array.from({ length: 500 }, (_, index) => dir(`Page0-${index}`))
    const pageTwoEntries = [dir('Page1-Alpha'), dir('Page1-Bravo')]
    ipc.override('begin_directory_session', (payload) => ({
      paneId: payload.paneId,
      tabId: payload.tabId,
      path: payload.path,
      baseline: { sessionId: 41, navigationRevision: 41, watchRevision: 0, viewRevision: 0 },
      totalRows: pageOneEntries.length + pageTwoEntries.length,
      pageSize: 500,
      firstPage: { pageIndex: 0, entries: pageOneEntries },
    }))
    ipc.override('get_directory_session_range', (payload) => ({
      baseline: payload.baseline,
      totalRows: pageOneEntries.length + pageTwoEntries.length,
      page: { pageIndex: payload.pageIndex, entries: pageTwoEntries },
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    const entries = usePanesStore.getState().panes.left.entries
    expect(entries).toHaveLength(502)
    expect(entries.slice(0, 3).map((entry) => entry.name)).toEqual([
      'Page0-0',
      'Page0-1',
      'Page0-2',
    ])
    expect(entries.slice(-2).map((entry) => entry.name)).toEqual(['Page1-Alpha', 'Page1-Bravo'])
  })

  it('ignores sort actions while a session begin is still pending', async () => {
    ipc.override('begin_directory_session', () => new Promise(() => {}) as never)
    void usePanesStore.getState().navigatePane('left', 'C:\\root')
    await vi.waitFor(() => expect(usePanesStore.getState().panes.left.loading).toBe(true))

    await usePanesStore.getState().setSort('left', 'items')
    const pane = usePanesStore.getState().panes.left
    expect(pane.sortKey).toBe('name')
    expect(pane.itemsSortStatus).toBe('idle')
    expect(pane.loading).toBe(true)
  })

  it('runs the backend Items sort only after the session finishes loading', async () => {
    let resolveSort:
      | ((value: {
          kind: 'ready'
          context: { paneId: string; tabId: string; requestId: number; path: string }
          path: string
          entries: DirectoryEntry[]
        }) => void)
      | undefined
    const sortActiveItems = vi.fn(
      () =>
        new Promise((resolve) => {
          resolveSort = resolve as typeof resolveSort
        }),
    )
    ipc.override('sort_active_items', sortActiveItems as never)
    ipc.override(
      'begin_directory_session',
      beginResponse({ entries: [dir('Alpha'), dir('Zulu')], sessionId: 9 }),
    )
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: { ...state.panes.left, sortKey: 'items', sortDirection: 'desc' },
      },
    }))

    // `reloadPane` awaits the Items sort inline as part of its own returned
    // promise (the sort only starts once the whole session has finished
    // loading), so this navigation is intentionally not awaited here — the
    // mocked `sort_active_items` above never resolves until `resolveSort` is
    // called below.
    void usePanesStore.getState().navigatePane('left', 'C:\\root')
    await vi.waitFor(() => expect(sortActiveItems).toHaveBeenCalledOnce())

    const tabId = useTabsStore.getState().panes.left.tabs[0].id
    const requestId = await vi.waitFor(() => {
      const currentRequestId = usePanesStore.getState().panes.left.listRequestId
      expect(currentRequestId).toBeGreaterThan(0)
      return currentRequestId
    })
    resolveSort?.({
      kind: 'ready',
      context: { paneId: 'left', tabId, requestId, path: 'C:\\root' },
      path: 'C:\\root',
      entries: [dir('Zulu'), dir('Alpha')],
    })
    await vi.waitFor(() =>
      expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
        'Zulu',
        'Alpha',
      ]),
    )
  })

  it('arms the watcher and eager-sizes directories once the session finishes loading', async () => {
    const setTabWatch = vi.fn(() => undefined)
    const requestFolderSizes = vi.fn(() => undefined)
    ipc.override('set_tab_watch', setTabWatch)
    ipc.override('request_folder_sizes', requestFolderSizes)
    ipc.override(
      'begin_directory_session',
      beginResponse({ entries: [dir('Alpha'), dir('Bravo')], sessionId: 3 }),
    )
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    const tabId = useTabsStore.getState().panes.left.tabs[0].id
    const requestId = usePanesStore.getState().panes.left.listRequestId

    await vi.waitFor(() => {
      expect(setTabWatch).toHaveBeenCalledWith(
        expect.objectContaining({
          target: expect.objectContaining({ path: 'C:\\root' }),
          seedReference: { tabId, requestId, path: 'C:\\root' },
        }),
      )
      expect(requestFolderSizes).toHaveBeenCalledWith({
        paths: ['C:\\root\\Alpha', 'C:\\root\\Bravo'],
      })
    })
  })

  it('re-sorts the loaded listing by size once the session finishes loading under size sort', async () => {
    ipc.override(
      'begin_directory_session',
      beginResponse({
        entries: [
          { ...dir('big.bin', false), sizeBytes: 900 },
          { ...dir('huge.bin', false), sizeBytes: 5000 },
          { ...dir('small.bin', false), sizeBytes: 10 },
        ],
        sessionId: 5,
      }),
    )
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: { ...state.panes.left, sortKey: 'size', sortDirection: 'desc' },
      },
    }))

    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'huge.bin',
      'big.bin',
      'small.bin',
    ])
  })

  it('ignores a stale resolved session begin after the pane navigates again', async () => {
    let resolveFirst: ((value: ReturnType<ReturnType<typeof beginResponse>>) => void) | undefined
    ipc.override(
      'begin_directory_session',
      vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise((resolve) => {
              resolveFirst = resolve
            }),
        )
        .mockImplementationOnce(beginResponse({ entries: [dir('Fresh')], sessionId: 2 })),
    )

    const firstNavigation = usePanesStore.getState().navigatePane('left', 'C:\\root')
    await Promise.resolve()
    await usePanesStore.getState().navigatePane('left', 'C:\\other')
    resolveFirst?.(
      beginResponse({ entries: [dir('Stale')], sessionId: 1, path: 'C:\\root' })({
        paneId: 'left',
        tabId: useTabsStore.getState().panes.left.tabs[0].id,
        path: 'C:\\root',
      }),
    )
    await firstNavigation

    expect(usePanesStore.getState().panes.left.path).toBe('C:\\other')
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Fresh',
    ])
  })

  it('ignores a stale listing error after the pane navigates again', async () => {
    let rejectFirst: ((reason?: unknown) => void) | undefined
    ipc.override(
      'begin_directory_session',
      vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise((_, reject) => {
              rejectFirst = reject
            }),
        )
        .mockImplementationOnce(beginResponse({ entries: [dir('Fresh')], sessionId: 2 })),
    )

    const firstNavigation = usePanesStore.getState().navigatePane('left', 'C:\\root')
    await Promise.resolve()
    await usePanesStore.getState().navigatePane('left', 'C:\\other')
    rejectFirst?.(new Error('Stale failure'))
    await firstNavigation

    expect(usePanesStore.getState().panes.left.path).toBe('C:\\other')
    expect(usePanesStore.getState().panes.left.error).toBeNull()
    expect(usePanesStore.getState().panes.left.loading).toBe(false)
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Fresh',
    ])
  })
})

describe('panes-store passive item counts', () => {
  it('requests only visible unknown-count directories within the bounded cap', async () => {
    const requestVisibleItemCounts = vi.fn((payload: unknown) => {
      void payload
      return undefined
    })
    ipc.override('request_visible_item_counts', requestVisibleItemCounts)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')

    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [
            ...Array.from({ length: 205 }, (_, index) => ({
              ...dirAt(`C:\\root\\Dir${index}`),
              itemCount: null,
            })),
            { ...dirAt('C:\\root\\readme.txt', false), itemCount: null },
            { ...dirAt('C:\\root\\Known'), itemCount: 4 },
          ],
        },
      },
    }))

    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 204, 204)
    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 204, 204)

    expect(requestVisibleItemCounts).toHaveBeenCalledOnce()
    const payload = requestVisibleItemCounts.mock.calls[0]?.[0] as
      | { context: unknown; paths: string[] }
      | undefined
    if (!payload) {
      throw new Error('missing request payload')
    }
    expect(payload.paths).toHaveLength(200)
    expect(requestVisibleItemCounts).toHaveBeenCalledWith({
      context: expect.objectContaining({
        paneId: 'left',
        tabId: expect.any(String),
        requestId: expect.any(Number),
        path: 'C:\\root',
      }),
      paths: Array.from({ length: 200 }, (_, index) => `C:\\root\\Dir${index}`),
    })
  })

  it('applies current item-count events without reordering rows or changing focus', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [
            { ...dirAt('C:\\root\\Zulu'), itemCount: null },
            { ...dirAt('C:\\root\\Alpha'), itemCount: null },
            dirAt('C:\\root\\file.txt', false),
          ],
          focusedEntryId: 'C:\\root\\Alpha',
        },
      },
    }))

    const beforeOrder = usePanesStore.getState().panes.left.entries.map((entry) => entry.name)
    const { listRequestId, path } = usePanesStore.getState().panes.left
    const tabId = useTabsStore.getState().panes.left.tabs[0].id
    usePanesStore.getState().applyItemCountEvents([
      {
        context: { paneId: 'left', tabId, requestId: listRequestId, path },
        results: [
          { path: 'C:\\root\\Alpha', itemCount: 2 },
          { path: 'C:\\root\\Zulu', itemCount: 7 },
        ],
        done: true,
      },
    ])

    const pane = usePanesStore.getState().panes.left
    expect(pane.entries.map((entry) => entry.name)).toEqual(beforeOrder)
    expect(pane.entries.find((entry) => entry.path === 'C:\\root\\Alpha')?.itemCount).toBe(2)
    expect(pane.entries.find((entry) => entry.path === 'C:\\root\\Zulu')?.itemCount).toBe(7)
    expect(pane.focusedEntryId).toBe('C:\\root\\Alpha')
  })

  it('re-arms passive counts after a watcher patch invalidates a known directory count', async () => {
    const requestVisibleItemCounts = vi.fn(() => undefined)
    ipc.override('request_visible_item_counts', requestVisibleItemCounts)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [{ ...dirAt('C:\\root\\Alpha'), itemCount: null }],
        },
      },
    }))

    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 0, 0)
    const pane = usePanesStore.getState().panes.left
    const tabId = useTabsStore.getState().panes.left.tabs[0].id
    usePanesStore.getState().applyItemCountEvents([
      {
        context: { paneId: 'left', tabId, requestId: pane.listRequestId, path: pane.path },
        results: [{ path: 'C:\\root\\Alpha', itemCount: 2 }],
        done: true,
      },
    ])
    usePanesStore.getState().applyDirPatch({
      tabId,
      path: 'C:\\root',
      reason: 'watch',
      changed: [
        { path: 'C:\\root\\Alpha', entry: { ...dirAt('C:\\root\\Alpha'), itemCount: null } },
      ],
      removed: [],
    })

    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 0, 0)
    expect(requestVisibleItemCounts).toHaveBeenCalledTimes(2)
  })

  it('retries a cancelled passive path after fast viewport supersession', async () => {
    const requestVisibleItemCounts = vi.fn(() => undefined)
    ipc.override('request_visible_item_counts', requestVisibleItemCounts)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [
            { ...dirAt('C:\\root\\Alpha'), itemCount: null },
            { ...dirAt('C:\\root\\Bravo'), itemCount: null },
          ],
        },
      },
    }))

    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 0, 0)
    await usePanesStore.getState().requestVisibleItemCounts('left', 1, 1, 0)
    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 0, 0)

    expect(requestVisibleItemCounts).toHaveBeenCalledTimes(3)
    const requestedPaths = (requestVisibleItemCounts.mock.calls as unknown as Array<[{ paths: string[] }]>).map(
      ([payload]) => payload.paths,
    )
    expect(requestedPaths).toEqual([
      ['C:\\root\\Alpha'],
      ['C:\\root\\Bravo'],
      ['C:\\root\\Alpha'],
    ])
  })

  it('does not request passive counts while an explicit Items sort is counting', async () => {
    const requestVisibleItemCounts = vi.fn(() => undefined)
    ipc.override('request_visible_item_counts', requestVisibleItemCounts)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          itemsSortStatus: 'counting',
          entries: [{ ...dirAt('C:\\root\\Alpha'), itemCount: null }],
        },
      },
    }))

    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 0, 0)
    expect(requestVisibleItemCounts).not.toHaveBeenCalled()
  })

  it('never requests passive filesystem counts for the trash virtual location', async () => {
    const requestVisibleItemCounts = vi.fn(() => undefined)
    ipc.override('request_visible_item_counts', requestVisibleItemCounts)
    await usePanesStore.getState().navigatePane('left', 'trash://')
    await usePanesStore.getState().requestVisibleItemCounts('left', 0, 10, 10)
    expect(requestVisibleItemCounts).not.toHaveBeenCalled()
  })

  it('ignores stale item-count events when tab, request, or path context no longer matches', async () => {
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [{ ...dirAt('C:\\root\\Alpha'), itemCount: null }],
        },
      },
    }))

    const pane = usePanesStore.getState().panes.left
    const currentTabId = useTabsStore.getState().panes.left.tabs[0].id
    usePanesStore.getState().applyItemCountEvents([
      {
        context: {
          paneId: 'left',
          tabId: currentTabId,
          requestId: pane.listRequestId + 1,
          path: pane.path,
        },
        results: [{ path: 'C:\\root\\Alpha', itemCount: 1 }],
        done: true,
      },
      {
        context: {
          paneId: 'left',
          tabId: 'left-tab-stale',
          requestId: pane.listRequestId,
          path: pane.path,
        },
        results: [{ path: 'C:\\root\\Alpha', itemCount: 2 }],
        done: true,
      },
      {
        context: {
          paneId: 'left',
          tabId: currentTabId,
          requestId: pane.listRequestId,
          path: 'C:\\other',
        },
        results: [{ path: 'C:\\root\\Alpha', itemCount: 3 }],
        done: true,
      },
    ])

    expect(usePanesStore.getState().panes.left.entries[0].itemCount).toBeNull()
  })
})

describe('panes-store active items sort', () => {
  it('counts then applies the final backend items order without progressive reordering', async () => {
    const sortActiveItems = vi.fn(() => ({
      kind: 'ready' as const,
      context: {
        paneId: 'left',
        tabId: useTabsStore.getState().panes.left.tabs[0].id,
        requestId: usePanesStore.getState().panes.left.listRequestId,
        path: 'C:\\root',
      },
      path: 'C:\\root',
      entries: [
        { ...dirAt('C:\\root\\Zulu'), itemCount: 7 },
        { ...dirAt('C:\\root\\Alpha'), itemCount: 2 },
        dirAt('C:\\root\\file.txt', false),
      ],
    }))
    ipc.override('sort_active_items', sortActiveItems)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [
            { ...dirAt('C:\\root\\Alpha'), itemCount: null },
            { ...dirAt('C:\\root\\Zulu'), itemCount: null },
            dirAt('C:\\root\\file.txt', false),
          ],
          focusedEntryId: 'C:\\root\\Alpha',
        },
      },
    }))

    await usePanesStore.getState().setSort('left', 'items')

    const pane = usePanesStore.getState().panes.left
    expect(sortActiveItems).toHaveBeenCalledOnce()
    expect(pane.entries.map((entry) => entry.name)).toEqual(['Zulu', 'Alpha', 'file.txt'])
    expect(pane.focusedEntryId).toBe('C:\\root\\Alpha')
    expect(pane.itemsSortStatus).toBe('complete')
  })

  it('rejects stale active items results after navigation, filter settle, or tab switch', async () => {
    let resolveSort:
      | ((value: {
          kind: 'ready'
          context: { paneId: string; tabId: string; requestId: number; path: string }
          path: string
          entries: DirectoryEntry[]
        }) => void)
      | undefined
    ipc.override(
      'sort_active_items',
      vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise((resolve) => {
              resolveSort = resolve as typeof resolveSort
            }),
        )
        .mockImplementation(() => ({
          kind: 'ready' as const,
          context: {
            paneId: 'left',
            tabId: useTabsStore.getState().panes.left.tabs[0].id,
            requestId: usePanesStore.getState().panes.left.listRequestId,
            path: usePanesStore.getState().panes.left.path,
          },
          path: usePanesStore.getState().panes.left.path,
          entries: [{ ...dirAt('C:\\other\\Alpha'), itemCount: 1 }, dirAt('C:\\other\\Beta')],
        })),
    )
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [{ ...dirAt('C:\\root\\Alpha'), itemCount: null }],
        },
      },
    }))

    const startingPane = usePanesStore.getState().panes.left
    const startingTabId = useTabsStore.getState().panes.left.tabs[0].id
    const sortPromise = usePanesStore.getState().setSort('left', 'items')
    await Promise.resolve()
    await usePanesStore.getState().navigatePane('left', 'C:\\other')
    resolveSort?.({
      kind: 'ready',
      context: {
        paneId: 'left',
        tabId: startingTabId,
        requestId: startingPane.listRequestId,
        path: 'C:\\root',
      },
      path: 'C:\\root',
      entries: [{ ...dirAt('C:\\root\\Zulu'), itemCount: 9 }],
    })
    await sortPromise

    expect(usePanesStore.getState().panes.left.path).toBe('C:\\other')
    expect(usePanesStore.getState().panes.left.entries.map((entry) => entry.name)).toEqual([
      'Alpha',
      'Beta',
    ])
  })

  it('keeps the newest same-context Items-sort terminal state when an older request fails', async () => {
    let resolveNewest: ((value: { kind: 'ready'; context: { paneId: string; tabId: string; requestId: number; path: string }; path: string; entries: DirectoryEntry[] }) => void) | undefined
    let rejectOldest: ((reason?: unknown) => void) | undefined
    ipc.override('sort_active_items', vi.fn()
      .mockImplementationOnce(() => new Promise((_, reject) => { rejectOldest = reject }))
      .mockImplementationOnce(() => new Promise((resolve) => { resolveNewest = resolve as typeof resolveNewest })))
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    const pane = usePanesStore.getState().panes.left
    const tabId = useTabsStore.getState().panes.left.tabs[0].id
    const first = usePanesStore.getState().setSort('left', 'items')
    await Promise.resolve()
    usePanesStore.getState().applyDirPatch({ tabId, path: pane.path, reason: 'watch', changed: [], removed: [] })
    await Promise.resolve()
    resolveNewest?.({ kind: 'ready', context: { paneId: 'left', tabId, requestId: pane.listRequestId, path: pane.path }, path: pane.path, entries: [{ ...dirAt('C:\\root\\Newest'), itemCount: 2 }] })
    await Promise.resolve()
    rejectOldest?.(new Error('old request failed'))
    await first
    const finalPane = usePanesStore.getState().panes.left
    expect(finalPane.entries.map((entry) => entry.name)).toEqual(['Newest'])
    expect(finalPane.itemsSortStatus).toBe('complete')
    expect(finalPane.error).toBeNull()
  })

  it('routes reloads and watcher invalidations through the active items sort path', async () => {
    const sortActiveItems = vi.fn(() => ({
      kind: 'ready' as const,
      context: {
        paneId: 'left',
        tabId: useTabsStore.getState().panes.left.tabs[0].id,
        requestId: usePanesStore.getState().panes.left.listRequestId,
        path: 'C:\\root',
      },
      path: 'C:\\root',
      entries: [
        { ...dirAt('C:\\root\\Newer'), itemCount: 11 },
        { ...dirAt('C:\\root\\Older'), itemCount: 3 },
      ],
    }))
    ipc.override('sort_active_items', sortActiveItems)
    await usePanesStore.getState().navigatePane('left', 'C:\\root')
    await usePanesStore.getState().setSort('left', 'items')
    sortActiveItems.mockClear()

    await usePanesStore.getState().reloadPane('left')
    expect(sortActiveItems).toHaveBeenCalledTimes(1)

    usePanesStore.getState().applyDirPatch({
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\root',
      reason: 'watch',
      changed: [
        { path: 'C:\\root\\Draft', entry: { ...dirAt('C:\\root\\Draft'), itemCount: null } },
      ],
      removed: [],
    })
    await Promise.resolve()
    expect(sortActiveItems).toHaveBeenCalledTimes(2)
    expect(usePanesStore.getState().panes.left.itemsSortStatus).toBe('counting')
  })

  it('uses the active items sort path for filter settles and preserves order on failures', async () => {
    vi.useFakeTimers()
    const sortActiveItems = vi.fn(() => {
      throw new Error('count failed')
    })
    ipc.override('sort_active_items', sortActiveItems)
    try {
      await usePanesStore.getState().navigatePane('left', 'C:\\root')
      usePanesStore.setState((state) => ({
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            sortKey: 'items',
            sortDirection: 'desc',
            itemsSortStatus: 'idle',
            entries: [
              { ...dirAt('C:\\root\\Alpha'), itemCount: null },
              { ...dirAt('C:\\root\\Zulu'), itemCount: null },
            ],
          },
        },
      }))

      usePanesStore.getState().setFilterDraft('left', 'Al')
      await vi.advanceTimersByTimeAsync(250)

      const pane = usePanesStore.getState().panes.left
      expect(sortActiveItems).toHaveBeenCalled()
      expect(pane.entries.map((entry) => entry.name)).toEqual(['Alpha', 'Beta'])
      expect(pane.itemsSortStatus).toBe('failed')
      expect(pane.error).toBe('count failed')
    } finally {
      vi.useRealTimers()
    }
  })
})
