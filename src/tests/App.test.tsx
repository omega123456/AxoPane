import { act, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { vi } from 'vitest'
import { ipc } from './ipc-mock'
import type { DirectoryEntry, ListDirRequest, ListDirResponse } from '@/lib/types/ipc'
import { renderApp } from './utils/render-app'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import { useQueueStore } from '@/stores/queue-store'

const rootEntries: DirectoryEntry[] = [
  {
    id: 'docs',
    name: 'Documents',
    path: 'C:\\Users\\Omega\\Documents',
    isDir: true,
    sizeBytes: null,
    itemCount: 12,
    typeLabel: 'Folder',
    modifiedAt: '2026-06-20T10:15:00Z',
    createdAt: '2026-06-01T10:15:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
  },
  {
    id: 'media',
    name: 'Media',
    path: 'C:\\Users\\Omega\\Media',
    isDir: true,
    sizeBytes: null,
    itemCount: 8,
    typeLabel: 'Folder',
    modifiedAt: '2026-06-21T10:15:00Z',
    createdAt: '2026-05-01T10:15:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
  },
  {
    id: 'report',
    name: 'Report.txt',
    path: 'C:\\Users\\Omega\\Report.txt',
    isDir: false,
    sizeBytes: 2048,
    itemCount: null,
    typeLabel: 'TXT file',
    modifiedAt: '2026-06-22T10:15:00Z',
    createdAt: '2026-06-10T10:15:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
  },
]

function installListDirOverride() {
  ipc.override('list_dir', (payload) => createDirResponse(payload))
  ipc.override('list_tree_children', (payload) => ({ path: payload.path, children: [] }))
  ipc.override('set_tab_watch', () => undefined)
}

function createDirResponse(payload: ListDirRequest): ListDirResponse {
  const isRootPath =
    payload.path === 'C:\\Users\\Omega' ||
    payload.path === 'D:\\projects' ||
    payload.path === 'C:\\' ||
    payload.path === 'D:\\'

  const entries = isRootPath ? rootEntries : []

  const filtered = entries.filter((entry) =>
    payload.filter ? entry.name.toLowerCase().includes(payload.filter.toLowerCase()) : true,
  )

  const sorted = [...filtered].sort((left, right) => {
    if (left.isDir !== right.isDir) {
      return left.isDir ? -1 : 1
    }

    if (payload.sortKey === 'modified') {
      const leftValue = left.modifiedAt ?? ''
      const rightValue = right.modifiedAt ?? ''
      return payload.sortDirection === 'asc'
        ? leftValue.localeCompare(rightValue)
        : rightValue.localeCompare(leftValue)
    }

    const leftValue = left.name.toLowerCase()
    const rightValue = right.name.toLowerCase()
    return payload.sortDirection === 'asc'
      ? leftValue.localeCompare(rightValue)
      : rightValue.localeCompare(leftValue)
  })

  return {
    path: payload.path,
    entries: sorted,
  }
}

function getRowInPane(paneLabel: string, name: string) {
  const pane = screen.getByLabelText(paneLabel)
  return within(pane)
    .getAllByRole('row')
    .find((row) => row.textContent?.includes(name))
}

describe('App', () => {
  it('renders both panes and the shared tree from IPC-backed data', async () => {
    installListDirOverride()
    renderApp()

    expect(await screen.findByLabelText('Left pane')).toBeInTheDocument()
    expect(await screen.findByLabelText('Right pane')).toBeInTheDocument()
    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })
  })

  it('toggles theme via the command bar button', async () => {
    const user = userEvent.setup()
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)
    installListDirOverride()
    renderApp()

    const toggle = await screen.findByRole('button', { name: 'Light theme' })
    expect(document.documentElement).toHaveClass('dark')

    await user.click(toggle)

    expect(document.documentElement).not.toHaveClass('dark')
    await waitFor(() => {
      expect(saveConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          config: expect.objectContaining({
            theme: 'light',
          }),
        }),
      )
    })
  })

  it('migrates a stale F5 refresh config and keeps F5 mapped to confirmed copy between panes', async () => {
    const user = userEvent.setup()
    const saveConfig = vi.fn((payload) => payload.config)
    const startOp = vi.fn(() => 'op-1')
    ipc.override('load_config', {
      theme: 'dark',
      showHiddenFiles: false,
      dismissedEverythingBanner: false,
      updateCheckInterval: '1d',
      logLevel: 'info',
      dateFormat: 'ymd',
      showTime: false,
      showSeconds: false,
      relativeDates: false,
      autoFolderSize: true,
      autoExpandActiveQueueToasts: false,
      keybindings: {
        refresh: ['F5'],
      },
      columns: [
        { key: 'name', visible: true },
        { key: 'size', visible: true },
        { key: 'items', visible: true },
        { key: 'type', visible: true },
        { key: 'modified', visible: true },
        { key: 'created', visible: false },
      ],
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
    })
    ipc.override('save_config', saveConfig)
    ipc.override('start_op', startOp)
    installListDirOverride()
    renderApp()

    const leftPane = await screen.findByLabelText('Left pane')
    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Report.txt')).toBeTruthy()
    })

    leftPane.focus()
    await user.keyboard('{ArrowDown}{ArrowDown}{F5}')
    const copyDialog = screen.getByRole('dialog', { name: 'Confirm copy' })
    expect(copyDialog).toBeInTheDocument()
    expect(within(copyDialog).getByText('C:\\Users\\Omega')).toBeInTheDocument()
    expect(within(copyDialog).getByText('D:\\projects')).toBeInTheDocument()
    expect(within(copyDialog).getByText('Report.txt')).toBeInTheDocument()
    expect(startOp).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Copy' }))

    expect(startOp).toHaveBeenCalledWith({
      kind: 'copy',
      destinationDir: 'D:\\projects',
      items: [
        {
          sourcePath: 'C:\\Users\\Omega\\Report.txt',
          name: 'Report.txt',
          sizeBytes: 2048,
        },
      ],
    })

    await waitFor(() => {
      expect(saveConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          config: expect.objectContaining({
            keybindings: expect.objectContaining({
              refresh: ['Ctrl+R'],
              copyToOtherPane: ['F5'],
            }),
          }),
        }),
      )
    })
  })

  it('toggles sorting when a header is clicked', async () => {
    const user = userEvent.setup()
    installListDirOverride()
    renderApp()

    const modifiedHeader = await within(screen.getByLabelText('Left pane')).findByRole('button', {
      name: /Modified/i,
    })
    await user.click(modifiedHeader)

    await waitFor(() => {
      const rows = within(screen.getByLabelText('Left pane'))
        .getAllByRole('row')
        .filter((row) => row.getAttribute('data-parent-row') !== 'true')
      expect(rows[0]).toHaveTextContent('Media')
    })
  })

  it('filters the active pane and clears with escape', async () => {
    const user = userEvent.setup()
    installListDirOverride()
    renderApp()

    const filter = await screen.findByRole('textbox', { name: 'Left pane filter' })
    await user.type(filter, 'Media')

    await waitFor(() => {
      expect(within(screen.getByLabelText('Left pane')).getByText('Media')).toBeInTheDocument()
      expect(
        within(screen.getByLabelText('Left pane')).queryByText('Documents'),
      ).not.toBeInTheDocument()
    })

    await user.type(filter, '{Escape}')

    await waitFor(() => {
      expect(within(screen.getByLabelText('Left pane')).getByText('Documents')).toBeInTheDocument()
    })
  })

  it('switches the active pane when Tab is pressed', async () => {
    const user = userEvent.setup()
    installListDirOverride()
    renderApp()

    const leftPane = await screen.findByLabelText('Left pane')
    const rightPane = await screen.findByLabelText('Right pane')

    leftPane.focus()
    expect(usePanesStore.getState().activePaneId).toBe('left')

    await user.keyboard('{Tab}')
    expect(usePanesStore.getState().activePaneId).toBe('right')
    expect(document.activeElement).toBe(rightPane)

    await user.keyboard('{Shift>}{Tab}{/Shift}')
    expect(usePanesStore.getState().activePaneId).toBe('left')
    expect(document.activeElement).toBe(leftPane)
  })

  it('navigates back and forward with the mouse side buttons', async () => {
    const user = userEvent.setup()
    installListDirOverride()
    renderApp()

    const leftPane = await screen.findByLabelText('Left pane')
    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })

    const startPath = usePanesStore.getState().panes.left.path
    leftPane.focus()
    await user.keyboard('{Enter}') // open the focused Documents folder

    await waitFor(() => {
      expect(usePanesStore.getState().panes.left.path).toBe('C:\\Users\\Omega\\Documents')
    })

    // Button 3 = browser "back".
    act(() => {
      window.dispatchEvent(new MouseEvent('mouseup', { button: 3 }))
    })
    await waitFor(() => {
      expect(usePanesStore.getState().panes.left.path).toBe(startPath)
    })

    // Button 4 = browser "forward".
    act(() => {
      window.dispatchEvent(new MouseEvent('mouseup', { button: 4 }))
    })
    await waitFor(() => {
      expect(usePanesStore.getState().panes.left.path).toBe('C:\\Users\\Omega\\Documents')
    })
  })

  it('ignores mouse side buttons that are not back/forward', async () => {
    installListDirOverride()
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })
    const startPath = usePanesStore.getState().panes.left.path

    act(() => {
      window.dispatchEvent(new MouseEvent('mouseup', { button: 1 }))
    })

    expect(usePanesStore.getState().panes.left.path).toBe(startPath)
  })

  it('supports click, ctrl-click, and shift-click selection', async () => {
    const user = userEvent.setup()
    installListDirOverride()
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })
    const documents = getRowInPane('Left pane', 'Documents')
    const media = getRowInPane('Left pane', 'Media')
    const report = getRowInPane('Left pane', 'Report.txt')

    if (!documents || !media || !report) {
      throw new Error('Rows not found')
    }

    await user.click(documents)
    await user.keyboard('[ControlLeft>]')
    await user.click(media)
    await user.keyboard('[/ControlLeft]')
    await user.keyboard('[ShiftLeft>]')
    await user.click(report)
    await user.keyboard('[/ShiftLeft]')

    expect(media.className).toContain('bg-accent-blue-soft')
    expect(report.className).toContain('bg-accent-blue-soft')
  })

  it('renders batched size state updates from the shared IPC event channel', async () => {
    installListDirOverride()
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })

    act(() => {
      ipc.emit('size://state', [
        {
          path: 'C:\\Users\\Omega\\Documents',
          state: 'calculating',
          source: 'everything',
          sizeBytes: null,
        },
        {
          path: 'C:\\Users\\Omega\\Documents',
          state: 'ready',
          source: 'everything',
          sizeBytes: 4096,
        },
      ])
    })

    await waitFor(() => {
      expect(getRowInPane('Left pane', '4.0 KB')).toBeTruthy()
    })
  })

  it('applies a batched icon://state event, patching every matched entry from one array payload', async () => {
    installListDirOverride()
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
      expect(getRowInPane('Left pane', 'Report.txt')).toBeTruthy()
    })

    act(() => {
      ipc.emit('icon://state', [
        { path: 'C:\\Users\\Omega\\Report.txt', iconDataUrl: 'data:image/png;base64,report-icon' },
      ])
    })

    await waitFor(() => {
      const row = getRowInPane('Left pane', 'Report.txt')
      const img = row?.querySelector('img[src="data:image/png;base64,report-icon"]')
      expect(img).toBeTruthy()
    })
  })

  it('updates the tree when mounted volumes are added or removed', async () => {
    installListDirOverride()
    renderApp()

    expect(await screen.findByRole('button', { name: 'Windows (C:)' })).toBeInTheDocument()
    expect(await screen.findByLabelText('Collapse Windows (C:)')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Archive (Y:)' })).not.toBeInTheDocument()

    act(() => {
      ipc.emit('volumes://changed', {
        volumes: [
          {
            mountRoot: 'C:\\',
            label: 'Windows',
            totalBytes: 1_000_000_000,
            freeBytes: 500_000_000,
            isNetwork: false,
            isRemovable: false,
          },
          {
            mountRoot: 'Y:\\',
            label: 'Archive',
            totalBytes: 750_000_000,
            freeBytes: 300_000_000,
            isNetwork: true,
            isRemovable: false,
          },
        ],
      })
    })

    expect(await screen.findByRole('button', { name: 'Archive (Y:)' })).toBeInTheDocument()
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'Projects (D:)' })).not.toBeInTheDocument()
      expect(screen.queryByRole('button', { name: 'USB Stick (E:)' })).not.toBeInTheDocument()
      expect(screen.queryByRole('button', { name: 'Share (Z:)' })).not.toBeInTheDocument()
    })
  })

  it('appends a streamed dir://list-chunk event onto the initial (partial) listing', async () => {
    ipc.override('set_tab_watch', () => undefined)
    ipc.override('list_tree_children', (payload) => ({ path: payload.path, children: [] }))
    // Left pane opens as a partial listing (first chunk only); the remainder
    // streams in as a separate list-chunk event.
    ipc.override('start_list_dir', (payload) => ({
      path: payload.path,
      total: payload.path === 'C:\\Users\\Omega' ? 3 : 0,
      requestId: 42,
      firstChunk:
        payload.path === 'C:\\Users\\Omega' ? [rootEntries[0], rootEntries[1]] : [],
      done: payload.path !== 'C:\\Users\\Omega',
    }))
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
      expect(getRowInPane('Left pane', 'Media')).toBeTruthy()
    })
    // The streamed remainder is not shown until its chunk arrives.
    expect(getRowInPane('Left pane', 'Report.txt')).toBeFalsy()

    const tabId = useTabsStore.getState().panes.left.tabs[0].id
    act(() => {
      ipc.emit('dir://list-chunk', {
        tabId,
        requestId: 42,
        path: 'C:\\Users\\Omega',
        entries: [rootEntries[2]],
        done: true,
      })
    })

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Report.txt')).toBeTruthy()
    })
  })

  it('shows a copy conflict application-wide instead of scoped to a single pane', async () => {
    const user = userEvent.setup()
    const resolveSpy = vi.fn(() => undefined)
    installListDirOverride()
    ipc.override('resolve_conflict', resolveSpy)
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })

    act(() => {
      useQueueStore.setState({
        conflicts: {
          'op-1': {
            operationId: 'op-1',
            sourcePath: 'C:\\src\\a.txt',
            destinationPath: 'D:\\dst\\a.txt',
            name: 'a.txt',
          },
        },
        order: ['op-1'],
      })
    })

    const dialog = await screen.findByRole('dialog', { name: 'Resolve file conflict' })
    expect(screen.getByLabelText('Left pane')).not.toContainElement(dialog)
    expect(screen.getByLabelText('Right pane')).not.toContainElement(dialog)

    await user.keyboard('{Enter}')
    expect(resolveSpy).toHaveBeenCalled()
  })
})
