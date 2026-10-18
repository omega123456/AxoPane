import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { PaneToolbar } from '@/components/pane/PaneToolbar'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneState } from '@/types/pane'

function folderEntries(count: number): DirectoryEntry[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `dir-${index}`,
    name: `dir-${index}`,
    path: `C:\\Users\\Omega\\dir-${index}`,
    isDir: true,
    sizeBytes: null,
    itemCount: 1,
    typeLabel: 'Folder',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }))
}

function pane(entries: DirectoryEntry[] = [], filterDraft = ''): PaneState {
  return {
    id: 'left',
    title: 'Left pane',
    path: 'C:\\Users\\Omega',
    entries,
    focusedEntryId: null,
    sortKey: 'name',
    sortDirection: 'asc',
    filterDraft,
    filterApplied: '',
    typing: false,
    loading: false,
    itemsSortStatus: 'idle',
    error: null,
    listRequestId: 0,
    scrollPositions: {},
  }
}

beforeEach(() => {
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useConfigStore.getState().reset()
  useActionDialogStore.getState().close()
})

describe('PaneToolbar', () => {
  it('wires pane navigation and refresh while showing the item count and filter', async () => {
    const user = userEvent.setup()
    const goBack = vi.fn(() => Promise.resolve())
    const goUp = vi.fn(() => Promise.resolve())
    const refreshSizes = vi.fn(() => Promise.resolve())
    usePanesStore.setState((state) => ({
      goBack,
      goUp,
      refreshSizes,
      panes: {
        ...state.panes,
        left: { ...state.panes.left, history: ['C:\\Users', 'C:\\Users\\Omega'], historyIndex: 1 },
      },
    }))

    render(<PaneToolbar pane={pane(folderEntries(2))} isActive />)

    expect(screen.getByRole('toolbar', { name: 'Left pane toolbar' })).toBeInTheDocument()
    expect(screen.getByText('2 items')).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: 'Left pane filter' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Back in Left pane' }))
    await user.click(screen.getByRole('button', { name: 'Up in Left pane' }))
    const refresh = screen.getByRole('button', { name: 'Refresh Left pane' })
    expect(refresh).toHaveAttribute('title', 'Refresh')
    await user.click(refresh)
    expect(goBack).toHaveBeenCalledWith('left')
    expect(goUp).toHaveBeenCalledWith('left')
    expect(refreshSizes).toHaveBeenCalledWith('left')
  })

  it('updates and clears the filter, returning focus to the pane on Escape', async () => {
    const user = userEvent.setup()
    const setFilterDraft = vi.fn()
    const clearFilter = vi.fn()
    usePanesStore.setState({ setFilterDraft, clearFilter })

    render(
      <div data-pane-id="left" tabIndex={0}>
        <PaneToolbar pane={pane()} isActive />
      </div>,
    )

    const filter = screen.getByRole('textbox', { name: 'Left pane filter' })
    await user.type(filter, 'x')
    expect(setFilterDraft).toHaveBeenCalledWith('left', 'x')
    await user.keyboard('{Escape}')
    expect(clearFilter).toHaveBeenCalledWith('left')
    expect(document.activeElement).toHaveAttribute('data-pane-id', 'left')
  })

  it('changes the active tab view through the existing View menu', async () => {
    const user = userEvent.setup()
    render(<PaneToolbar pane={pane()} isActive />)

    await user.click(screen.getByRole('button', { name: 'View: Details' }))
    await user.click(screen.getByRole('menuitemradio', { name: 'Icons' }))

    const tabs = useTabsStore.getState().panes.left
    expect(tabs.tabs[tabs.activeTabIndex]?.viewMode).toBe('icons')
  })

  it('shows the size action when needed and opens its confirmation dialog', async () => {
    const user = userEvent.setup()
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'everything', isAvailable: false, state: 'unavailable' },
    })

    render(<PaneToolbar pane={pane()} isActive />)
    const button = screen.getByRole('button', {
      name: 'Calculate all folder sizes in Left pane',
    })
    expect(button).toHaveAttribute('title', 'Calculate all folder sizes')
    await user.click(button)

    expect(useActionDialogStore.getState().dialog).toEqual({
      kind: 'calculateAllSizes',
      paneId: 'left',
    })
  })

  it('opens the create folder and file dialogs', async () => {
    const user = userEvent.setup()
    render(<PaneToolbar pane={pane()} isActive />)

    await user.click(screen.getByRole('button', { name: 'Create new folder in Left pane' }))
    expect(useActionDialogStore.getState().dialog).toEqual({ kind: 'newFolder', paneId: 'left' })

    await user.click(screen.getByRole('button', { name: 'Create new file in Left pane' }))
    expect(useActionDialogStore.getState().dialog).toEqual({ kind: 'newFile', paneId: 'left' })
  })

  it('uses availability, configuration, and the folder threshold for size-action visibility', () => {
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'macIndex', isAvailable: true, state: 'upToDate' },
    })
    useConfigStore.setState({ autoFolderSize: true })

    const { rerender } = render(<PaneToolbar pane={pane(folderEntries(500))} isActive />)
    const buttonName = 'Calculate all folder sizes in Left pane'
    expect(screen.queryByRole('button', { name: buttonName })).not.toBeInTheDocument()

    rerender(<PaneToolbar pane={pane(folderEntries(501))} isActive />)
    expect(screen.getByRole('button', { name: buttonName })).toBeInTheDocument()

    act(() => useConfigStore.setState({ autoFolderSize: false }))
    rerender(<PaneToolbar pane={pane()} isActive />)
    expect(screen.getByRole('button', { name: buttonName })).toBeInTheDocument()
  })

  it('shows the size action when automatic sizing left a folder unknown', () => {
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'macIndex', isAvailable: true, state: 'upToDate' },
    })
    const entries = folderEntries(2)
    const buttonName = 'Calculate all folder sizes in Left pane'
    render(<PaneToolbar pane={pane(entries)} isActive />)
    expect(screen.queryByRole('button', { name: buttonName })).not.toBeInTheDocument()

    act(() =>
      usePanesStore
        .getState()
        .applySizeStates([
          { path: entries[1].path, state: 'unknown', source: 'index', sizeBytes: null },
        ]),
    )
    expect(screen.getByRole('button', { name: buttonName })).toBeInTheDocument()
  })

  it('shows a catch-up spinner next to the item count while folder sizes update', () => {
    const entries = folderEntries(2)
    render(<PaneToolbar pane={pane(entries)} isActive />)
    expect(screen.queryByText('Updating folder sizes')).not.toBeInTheDocument()

    const ready = { state: 'ready', source: 'index', sizeBytes: 5 } as const
    act(() =>
      usePanesStore.getState().applySizeStates([
        { path: entries[0].path, ...ready },
        { path: entries[1].path, state: 'calculating', source: 'index', sizeBytes: null },
      ]),
    )
    expect(screen.getByText('Updating folder sizes')).toHaveClass('sr-only')

    act(() => usePanesStore.getState().applySizeStates([{ path: entries[1].path, ...ready }]))
    expect(screen.queryByText('Updating folder sizes')).not.toBeInTheDocument()

    // A refresh keeps the value but still counts as updating.
    act(() =>
      usePanesStore
        .getState()
        .applySizeStates([
          { path: entries[0].path, state: 'calculating', source: 'index', sizeBytes: null },
        ]),
    )
    expect(screen.getByText('Updating folder sizes')).toBeInTheDocument()
  })
})
