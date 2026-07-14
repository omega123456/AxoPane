import { createEvent, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { DetailsPanel } from '@/components/details/DetailsPanel'
import { HeaderRow, columnFlexStyle, paneContentWidth } from '@/components/pane/HeaderRow'
import { SizeValue } from '@/components/pane/SizeValue'
import { TreeNode, type TreeRowActions } from '@/components/tree/TreeNode'
import { TRASH_PATH } from '@/lib/trash'
import { useDragStore } from '@/stores/drag-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore, type TreeNodeState } from '@/stores/panes-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
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
  useLayoutStore.getState().reset()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  usePropertiesDialogStore.getState().close()
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
    await waitFor(() =>
      expect(usePropertiesDialogStore.getState().dialog).toMatchObject({
        items: [expect.objectContaining({ path: 'C:\\Users\\Omega\\Report.txt' })],
      }),
    )
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
    const nameHeader = screen.getByRole('button', { name: /Name/ })
    const sizeHeader = screen.getByRole('button', { name: /Size/ })

    expect(nameHeader).toHaveClass('w-full')
    expect(sizeHeader).not.toHaveClass('justify-end')
    expect(sizeHeader.querySelector('svg')).toHaveClass('ml-auto')

    await user.click(nameHeader)
    expect(setSort).toHaveBeenCalledWith('left', 'name')
  })

  it('keeps every column fixed-width so resizing one column leaves the others alone', () => {
    expect(columnFlexStyle('name', { name: 384 })).toEqual({
      flex: '0 0 384px',
      width: '384px',
    })
    expect(columnFlexStyle('type', { type: 184 })).toEqual({
      flex: '0 0 184px',
      width: '184px',
    })
    expect(
      paneContentWidth(
        [
          { key: 'name', visible: true },
          { key: 'size', visible: true },
        ],
        { name: 384, size: 96 },
      ),
    ).toBe(516)
  })

  it('resizes a column by dragging its header divider', () => {
    const pane = usePanesStore.getState().panes.left
    render(<HeaderRow pane={{ ...pane, sortKey: 'type', sortDirection: 'asc' }} />)

    const divider = screen.getByRole('separator', { name: 'Resize Type column' })
    fireEvent.pointerDown(divider, { button: 0, clientX: 100, pointerId: 1 })
    fireEvent.pointerMove(divider, { clientX: 140, pointerId: 1 })
    fireEvent.pointerUp(divider, { pointerId: 1 })

    expect(useLayoutStore.getState().columnWidths.type).toBe(176)
  })

  it('relabels the Modified column to Deleted when browsing the trash pane', () => {
    const pane = usePanesStore.getState().panes.left
    render(
      <HeaderRow pane={{ ...pane, path: TRASH_PATH, sortKey: 'modified', sortDirection: 'asc' }} />,
    )

    expect(screen.getByRole('button', { name: /Deleted/ })).toBeInTheDocument()
    expect(screen.getByRole('separator', { name: 'Resize Deleted column' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^Modified/ })).not.toBeInTheDocument()
  })
})

function treeNodeState(overrides: Partial<TreeNodeState> = {}): TreeNodeState {
  return {
    id: 'C:\\root',
    name: 'root',
    path: 'C:\\root',
    parentPath: null,
    children: [],
    expanded: true,
    loaded: true,
    ...overrides,
  }
}

/** A dragLeave event with a reliably-set `relatedTarget` (fireEvent init leaves it read-only/null). */
function dragLeaveTo(row: HTMLElement, relatedTarget: Element) {
  const event = createEvent.dragLeave(row)
  Object.defineProperty(event, 'relatedTarget', { value: relatedTarget })
  return event
}

function treeRowActions(overrides: Partial<TreeRowActions> = {}): TreeRowActions {
  return {
    onToggle: vi.fn(),
    onNavigate: vi.fn(),
    onOpenTab: vi.fn(),
    onContextMenu: vi.fn(),
    ...overrides,
  }
}

describe('TreeNode', () => {
  it('renders the node label and marks the current node opaque when pinned', () => {
    const { rerender } = render(
      <TreeNode node={treeNodeState()} depth={0} isCurrent actions={treeRowActions()} />,
    )
    const label = screen.getByText('root')
    expect(label).toBeInTheDocument()
    // Current + in-flow uses the translucent selection tint.
    expect(label.closest('[data-tree-row]')?.className).toContain('bg-accent-blue-soft')

    // Current + pinned swaps to the opaque stand-in so rows scrolling under it
    // don't bleed through.
    rerender(
      <TreeNode node={treeNodeState()} depth={0} isCurrent isPinned actions={treeRowActions()} />,
    )
    expect(screen.getByText('root').closest('[data-tree-row]')?.className).toContain(
      'bg-light-tree-current',
    )
  })

  it('expands and navigates through the stable actions', async () => {
    const user = userEvent.setup()
    const actions = treeRowActions()
    render(<TreeNode node={treeNodeState()} depth={0} isCurrent actions={actions} />)

    await user.click(screen.getByRole('button', { name: /Collapse root/ }))
    expect(actions.onToggle).toHaveBeenCalledWith('C:\\root')

    await user.click(screen.getByText('root'))
    expect(actions.onNavigate).toHaveBeenCalledWith('C:\\root')
  })

  it('opens a node in a new tab via middle-click', async () => {
    const user = userEvent.setup()
    const actions = treeRowActions()
    render(<TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={actions} />)

    await user.pointer({ keys: '[MouseMiddle]', target: screen.getByText('root') })
    expect(actions.onOpenTab).toHaveBeenCalledWith('C:\\root')
  })

  it('opens a node in a new tab when middle-clicking its icon', async () => {
    const user = userEvent.setup()
    const actions = treeRowActions()
    render(<TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={actions} />)

    const icon = screen.getByText('root').closest('button')!.querySelector('svg')!
    await user.pointer({ keys: '[MouseMiddle]', target: icon })
    expect(actions.onOpenTab).toHaveBeenCalledWith('C:\\root')
  })

  it('suppresses middle-click autoscroll so the auxclick gesture can fire', () => {
    render(
      <TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={treeRowActions()} />,
    )

    const label = screen.getByText('root').closest('button')!
    const prevented = !fireEvent.mouseDown(label, { button: 1 })
    expect(prevented).toBe(true)

    // Left-button mousedown must not be prevented (preserves focus/click).
    const leftAllowed = fireEvent.mouseDown(label, { button: 0 })
    expect(leftAllowed).toBe(true)
  })

  it('highlights on a valid drag-over and clears only when the pointer truly leaves', () => {
    useDragStore.getState().begin({
      kind: 'file-transfer',
      sourcePaneId: 'left',
      sourceDir: 'C:\\other',
      items: [{ id: 'a', name: 'A', path: 'C:\\other\\A', isDir: false, sizeBytes: 1 }],
    })
    render(
      <TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={treeRowActions()} />,
    )
    const row = screen.getByRole('treeitem')

    fireEvent.dragOver(row, { dataTransfer: { dropEffect: '' } })
    expect(row).toHaveClass('ring-accent-blue-border')

    // Moving onto a child element inside the row is not a leave.
    fireEvent(row, dragLeaveTo(row, within(row).getAllByRole('button')[0]))
    expect(row).toHaveClass('ring-accent-blue-border')

    fireEvent(row, dragLeaveTo(row, document.body))
    expect(row).not.toHaveClass('ring-accent-blue-border')

    useDragStore.getState().end()
  })

  it('ignores a drag-over when nothing is being dragged', () => {
    useDragStore.getState().end()
    render(
      <TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={treeRowActions()} />,
    )
    const row = screen.getByRole('treeitem')

    fireEvent.dragOver(row)
    expect(row).not.toHaveClass('ring-accent-blue-border')
  })

  it('reads macOS drop modifiers when computing the drop effect', () => {
    const originalPlatform = navigator.platform
    Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true })
    useDragStore.getState().begin({
      kind: 'file-transfer',
      sourcePaneId: 'left',
      sourceDir: 'C:\\other',
      items: [{ id: 'a', name: 'A', path: 'C:\\other\\A', isDir: false, sizeBytes: 1 }],
    })
    render(
      <TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={treeRowActions()} />,
    )
    const row = screen.getByRole('treeitem')

    fireEvent.dragOver(row, { altKey: true, dataTransfer: { dropEffect: '' } })
    expect(row).toHaveClass('ring-accent-blue-border')

    useDragStore.getState().end()
    Object.defineProperty(navigator, 'platform', { value: originalPlatform, configurable: true })
  })

  it('opens the context menu through the stable action', () => {
    const actions = treeRowActions()
    render(<TreeNode node={treeNodeState()} depth={0} isCurrent={false} actions={actions} />)

    fireEvent.contextMenu(screen.getByRole('treeitem'))
    expect(actions.onContextMenu).toHaveBeenCalledWith(
      expect.objectContaining({ path: 'C:\\root' }),
      expect.anything(),
    )
  })
})
