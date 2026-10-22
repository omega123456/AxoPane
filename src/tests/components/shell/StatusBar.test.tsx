import { act, render, screen } from '@testing-library/react'
import { beforeEach } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { StatusBar } from '@/components/shell/StatusBar'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { DirectoryEntry } from '@/lib/types/ipc'

const reportEntry: DirectoryEntry = {
  id: 'r',
  name: 'Report.txt',
  path: 'C:\\Users\\Report.txt',
  isDir: false,
  sizeBytes: 1024,
  itemCount: null,
  typeLabel: 'TXT file',
  modifiedAt: null,
  createdAt: null,
  attributes: [],
  isHidden: false,
  isSystem: false,
}

const docsEntry: DirectoryEntry = {
  id: 'd',
  name: 'Docs',
  path: 'C:\\Users\\Docs',
  isDir: true,
  sizeBytes: null,
  itemCount: 2,
  typeLabel: 'Folder',
  modifiedAt: null,
  createdAt: null,
  attributes: [],
  isHidden: false,
  isSystem: false,
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useSelectionStore.getState().reset()
  useConfigStore.setState({ autoFolderSize: true })
})

function selectAll(indexAvailable: boolean) {
  act(() => {
    usePanesStore.setState((state) => ({
      activePaneId: 'left',
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\Users',
          typing: false,
          entries: [reportEntry, docsEntry],
          focusedEntryId: 'r',
        },
      },
      volumes: [],
      folderSizeIndexStatus: {
        backend: 'everything',
        isAvailable: indexAvailable,
        state: indexAvailable ? 'upToDate' : 'unavailable',
      },
      sizeStates: {
        [docsEntry.path]: { state: 'ready', source: 'everything', sizeBytes: 2048 },
      },
    }))
    useSelectionStore.setState((state) => ({
      selections: {
        ...state.selections,
        left: { ...state.selections.left, selectedIds: ['r', 'd'] },
      },
    }))
  })
}

describe('StatusBar', () => {
  it('renders counts, focused entry, and volume free space from the stores', () => {
    act(() => {
      usePanesStore.setState((state) => ({
        activePaneId: 'left',
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            path: 'C:\\Users',
            typing: false,
            entries: [reportEntry, docsEntry, reportEntry],
            focusedEntryId: 'r',
          },
        },
        volumes: [
          {
            mountRoot: 'C:\\',
            label: 'System',
            totalBytes: 4_000_000_000_000,
            freeBytes: 412_000_000_000,
            isNetwork: false,
            isRemovable: false,
          },
        ],
      }))
      useSelectionStore.setState((state) => ({
        selections: {
          ...state.selections,
          left: { ...state.selections.left, selectedIds: ['r'] },
        },
      }))
    })

    render(<StatusBar />)

    expect(screen.getByText('3 items')).toBeInTheDocument()
    expect(screen.getByText('1 selected')).toBeInTheDocument()
    expect(screen.getByText(/Report.txt/)).toBeInTheDocument()
    expect(screen.getByText(/free of/)).toBeInTheDocument()
  })

  it('shows a filtering indicator and a folder focus label without a volume', () => {
    act(() => {
      usePanesStore.setState((state) => ({
        activePaneId: 'left',
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            path: 'C:\\Users',
            typing: true,
            entries: [docsEntry],
            focusedEntryId: 'd',
          },
        },
        volumes: [],
      }))
    })

    render(<StatusBar />)

    expect(screen.getByText('Filtering…')).toBeInTheDocument()
    expect(screen.getByText(/· folder/)).toBeInTheDocument()
    expect(screen.getByText('0 selected')).toBeInTheDocument()
  })

  it('shows terse counting feedback while explicit items sorting is pending', () => {
    act(() => {
      usePanesStore.setState((state) => ({
        activePaneId: 'left',
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            path: 'C:\\Users',
            sortKey: 'items',
            itemsSortStatus: 'counting',
            entries: [docsEntry],
          },
        },
      }))
    })

    render(<StatusBar />)

    expect(screen.getByRole('status', { name: 'Counting items…' })).toBeInTheDocument()
  })

  it('follows the active pane when it changes', () => {
    act(() => {
      usePanesStore.setState((state) => ({
        activePaneId: 'left',
        panes: {
          left: { ...state.panes.left, path: 'C:\\Left', typing: false, entries: [] },
          right: { ...state.panes.right, path: 'C:\\Right', typing: false, entries: [reportEntry] },
        },
      }))
    })

    render(<StatusBar />)
    expect(screen.getByText('C:\\Left')).toBeInTheDocument()
    expect(screen.getByText('0 items')).toBeInTheDocument()

    act(() => {
      usePanesStore.setState({ activePaneId: 'right' })
    })

    expect(screen.getByText('C:\\Right')).toBeInTheDocument()
    expect(screen.getByText('1 items')).toBeInTheDocument()
  })

  it('totals selected files only when no folder-size index is available', () => {
    selectAll(false)

    render(<StatusBar />)

    expect(screen.getByText('2 selected')).toBeInTheDocument()
    expect(screen.getByText('1.0 KB')).toBeInTheDocument()
  })

  it('adds indexed folder sizes to the selection total', () => {
    selectAll(true)

    render(<StatusBar />)

    expect(screen.getByText('3.0 KB')).toBeInTheDocument()
  })

  it('leaves folder sizes out when automatic folder sizing is off', () => {
    selectAll(true)
    useConfigStore.setState({ autoFolderSize: false })

    render(<StatusBar />)

    expect(screen.getByText('1.0 KB')).toBeInTheDocument()
  })

  it('omits the size when no selected item has a known size', () => {
    selectAll(false)
    act(() => {
      useSelectionStore.setState((state) => ({
        selections: { ...state.selections, left: { ...state.selections.left, selectedIds: ['d'] } },
      }))
    })

    render(<StatusBar />)

    expect(screen.getByText('1 selected')).toBeInTheDocument()
    expect(screen.queryByText(/ B$/)).not.toBeInTheDocument()
  })

  it('hides the focused entry name while several items are selected', () => {
    selectAll(true)

    render(<StatusBar />)

    expect(screen.queryByText(/Report.txt/)).not.toBeInTheDocument()
  })
})
