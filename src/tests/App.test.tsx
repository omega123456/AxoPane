import { act, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { vi } from 'vitest'
import { ipc } from './ipc-mock'
import type { DirectoryEntry, ListDirRequest, ListDirResponse } from '@/lib/types/ipc'
import { renderApp } from './utils/render-app'
import { usePanesStore } from '@/stores/panes-store'

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
    expect(await screen.findByText('Shared tree')).toBeInTheDocument()
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
        treeWidth: 'default',
        defaultPaneMode: 'dual',
        restoreSession: true,
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

  it('renders size state updates from the shared IPC event channel', async () => {
    installListDirOverride()
    renderApp()

    await waitFor(() => {
      expect(getRowInPane('Left pane', 'Documents')).toBeTruthy()
    })

    act(() => {
      ipc.emit('size://state', {
        path: 'C:\\Users\\Omega\\Documents',
        state: 'calculating',
        source: 'everything',
        sizeBytes: null,
      })
    })

    act(() => {
      ipc.emit('size://state', {
        path: 'C:\\Users\\Omega\\Documents',
        state: 'ready',
        source: 'everything',
        sizeBytes: 4096,
      })
    })

    await waitFor(() => {
      expect(getRowInPane('Left pane', '4.0 KB')).toBeTruthy()
    })
  })
})
