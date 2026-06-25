import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { DetailsPanel } from '@/components/details/DetailsPanel'
import { HeaderRow } from '@/components/pane/HeaderRow'
import { SizeValue } from '@/components/pane/SizeValue'
import { TreeNode } from '@/components/tree/TreeNode'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { DirectoryEntry } from '@/lib/types/ipc'

function entry(overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    id: 'docs',
    name: 'Documents',
    path: 'C:\\Users\\Omega\\Documents',
    isDir: true,
    sizeBytes: null,
    itemCount: 4,
    typeLabel: 'Folder',
    modifiedAt: '2026-06-20T10:15:00Z',
    createdAt: '2026-06-01T10:15:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
    ...overrides,
  }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
})

describe('DetailsPanel', () => {
  it('renders the no-selection placeholder for an empty pane', () => {
    render(<DetailsPanel paneId="left" />)
    expect(screen.getByText('No selection')).toBeInTheDocument()
  })

  it('renders details and acts on the action buttons', async () => {
    const user = userEvent.setup()
    const writeText = vi.fn(() => Promise.resolve())
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    })
    const openPath = vi.fn(() => undefined)
    ipc.override('open_path', openPath)

    const file = entry({
      id: 'r',
      name: 'Report.txt',
      path: 'C:\\Users\\Omega\\Report.txt',
      isDir: false,
      typeLabel: 'TXT file',
    })
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: { ...state.panes.left, entries: [file], focusedEntryId: 'r' },
      },
    }))

    render(<DetailsPanel paneId="left" />)
    expect(screen.getByText('Report.txt')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Open' }))
    await user.click(screen.getByRole('button', { name: 'Copy path' }))
    await user.click(screen.getByRole('button', { name: 'Properties' }))

    expect(openPath).toHaveBeenCalledWith({ path: 'C:\\Users\\Omega\\Report.txt' })
    expect(writeText).toHaveBeenCalledWith('C:\\Users\\Omega\\Report.txt')
  })

  it('navigates when opening a focused folder', async () => {
    const user = userEvent.setup()
    const navigate = vi.fn(() => Promise.resolve())
    usePanesStore.setState((state) => ({
      navigatePane: navigate,
      panes: {
        ...state.panes,
        left: { ...state.panes.left, entries: [entry()], focusedEntryId: 'docs' },
      },
    }))

    render(<DetailsPanel paneId="left" />)
    await user.click(screen.getByRole('button', { name: 'Open' }))
    expect(navigate).toHaveBeenCalledWith('left', 'C:\\Users\\Omega\\Documents')
  })
})

describe('SizeValue', () => {
  it('renders each folder size state and file sizes', () => {
    usePanesStore.setState({
      sizeStates: {
        'C:\\calc': { state: 'calculating', sizeBytes: null, source: 'manual' },
        'C:\\ready': { state: 'ready', sizeBytes: 2048, source: 'everything' },
        'C:\\na': { state: 'na', sizeBytes: null, source: 'network' },
      },
    })

    const { rerender, container } = render(<SizeValue entry={entry({ path: 'C:\\calc' })} />)
    expect(container.querySelector('svg')).toBeInTheDocument()

    rerender(<SizeValue entry={entry({ path: 'C:\\ready' })} />)
    expect(screen.getByText('2.0 KB')).toBeInTheDocument()

    rerender(<SizeValue entry={entry({ path: 'C:\\na' })} />)
    expect(screen.getByText('N/A')).toBeInTheDocument()

    rerender(<SizeValue entry={entry({ path: 'C:\\unknown' })} />)
    expect(screen.getByText('—')).toBeInTheDocument()

    rerender(<SizeValue entry={entry({ isDir: false, path: 'C:\\file', sizeBytes: 1048576 })} />)
    expect(screen.getByText('1.0 MB')).toBeInTheDocument()

    rerender(<SizeValue entry={entry({ isDir: false, path: 'C:\\empty', sizeBytes: null })} />)
    expect(screen.getByText('—')).toBeInTheDocument()
  })
})

describe('HeaderRow', () => {
  it('marks the active sort column and triggers setSort', async () => {
    const user = userEvent.setup()
    const setSort = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ setSort })
    const pane = usePanesStore.getState().panes.left

    render(<HeaderRow pane={{ ...pane, sortKey: 'size', sortDirection: 'desc' }} />)
    await user.click(screen.getByRole('button', { name: /Name/ }))
    expect(setSort).toHaveBeenCalledWith('left', 'name')
  })
})

describe('TreeNode', () => {
  it('renders nothing for an unknown node', () => {
    const { container } = render(
      <ul>
        <TreeNode path="C:\\missing" depth={0} />
      </ul>,
    )
    expect(container.querySelector('li')).toBeNull()
  })

  it('expands, navigates, and marks the current node', async () => {
    const user = userEvent.setup()
    const toggle = vi.fn(() => Promise.resolve())
    const navigate = vi.fn(() => Promise.resolve())
    usePanesStore.setState((state) => ({
      toggleTreeNode: toggle,
      navigatePane: navigate,
      panes: { ...state.panes, left: { ...state.panes.left, path: 'C:\\root' } },
      treeNodes: {
        'C:\\root': {
          id: 'C:\\root',
          name: 'root',
          path: 'C:\\root',
          parentPath: null,
          children: [],
          expanded: true,
          loaded: true,
        },
      },
    }))

    render(
      <ul>
        <TreeNode path={'C:\\root'} depth={0} />
      </ul>,
    )

    await user.click(screen.getByRole('button', { name: /Collapse root/ }))
    expect(toggle).toHaveBeenCalledWith('C:\\root')

    await user.click(screen.getByText('root'))
    expect(navigate).toHaveBeenCalledWith('left', 'C:\\root')
  })
})
