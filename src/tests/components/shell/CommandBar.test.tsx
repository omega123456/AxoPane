import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { CommandBar } from '@/components/shell/CommandBar'
import { registerPaneFilterKeys } from '@/lib/pane-filter-keys'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

type StorePane = ReturnType<typeof usePanesStore.getState>['panes']['left']

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

function seedPane(paneId: PaneId, partial: Partial<StorePane>) {
  act(() => {
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, [paneId]: { ...state.panes[paneId], ...partial } },
    }))
  })
}

function renderBar(theme: 'light' | 'dark' = 'light', setTheme = vi.fn()) {
  return render(
    <>
      <CommandBar theme={theme} setTheme={setTheme} />
      <section data-pane-id="left" tabIndex={0} aria-label="Left pane" />
      <section data-pane-id="right" tabIndex={0} aria-label="Right pane" />
    </>,
  )
}

const calculateName = 'Calculate all folder sizes in Left pane'

beforeEach(() => {
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useConfigStore.getState().reset()
  useActionDialogStore.getState().close()
  useSettingsStore.getState().close()
})

describe('CommandBar', () => {
  it('runs Back, Up, Refresh, ~ and / against the active pane', async () => {
    const user = userEvent.setup()
    const goBack = vi.fn(() => Promise.resolve())
    const goUp = vi.fn(() => Promise.resolve())
    const refreshSizes = vi.fn(() => Promise.resolve())
    const navigatePane = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ goBack, goUp, refreshSizes, navigatePane, activePaneId: 'right' })
    seedPane('right', { history: ['D:\\', 'D:\\src'], historyIndex: 1 })

    renderBar()

    expect(screen.getByRole('toolbar', { name: 'Command bar' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Back in Right pane' }))
    await user.click(screen.getByRole('button', { name: 'Up in Right pane' }))
    const refresh = screen.getByRole('button', { name: 'Refresh Right pane' })
    expect(refresh).toHaveAttribute('title', 'Refresh')
    await user.click(refresh)
    await user.click(screen.getByRole('button', { name: 'Navigate to ~' }))
    await user.click(screen.getByRole('button', { name: 'Navigate to /' }))

    expect(goBack).toHaveBeenCalledWith('right')
    expect(goUp).toHaveBeenCalledWith('right')
    expect(refreshSizes).toHaveBeenCalledWith('right')
    expect(navigatePane).toHaveBeenNthCalledWith(1, 'right', '~')
    expect(navigatePane).toHaveBeenNthCalledWith(2, 'right', '/')
  })

  it('disables Back with no history', () => {
    renderBar()

    expect(screen.getByRole('button', { name: 'Back in Left pane' })).toBeDisabled()
  })

  it('opens the create folder and file dialogs for the active pane', async () => {
    const user = userEvent.setup()
    renderBar()

    await user.click(screen.getByRole('button', { name: 'Create new folder in Left pane' }))
    expect(useActionDialogStore.getState().dialog).toEqual({ kind: 'newFolder', paneId: 'left' })

    await user.click(screen.getByRole('button', { name: 'Create new file in Left pane' }))
    expect(useActionDialogStore.getState().dialog).toEqual({ kind: 'newFile', paneId: 'left' })
  })

  it('changes the active tab view through the View menu', async () => {
    const user = userEvent.setup()
    renderBar()

    await user.click(screen.getByRole('button', { name: 'View: Details' }))
    await user.click(screen.getByRole('menuitemradio', { name: 'Icons' }))

    const tabs = useTabsStore.getState().panes.left
    expect(tabs.tabs[tabs.activeTabIndex]?.viewMode).toBe('icons')
  })

  it('shows the size action when sizing is unavailable and opens its confirmation', async () => {
    const user = userEvent.setup()
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'everything', isAvailable: false, state: 'unavailable' },
    })
    renderBar()

    const button = screen.getByRole('button', { name: calculateName })
    expect(button).toHaveAttribute('title', 'Calculate all folder sizes')
    await user.click(button)

    expect(useActionDialogStore.getState().dialog).toEqual({
      kind: 'calculateAllSizes',
      paneId: 'left',
    })
  })

  it('uses availability, configuration and the folder threshold for the size action', () => {
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'macIndex', isAvailable: true, state: 'upToDate' },
    })
    act(() => useConfigStore.setState({ autoFolderSize: true }))
    seedPane('left', { entries: folderEntries(500) })
    renderBar()
    expect(screen.queryByRole('button', { name: calculateName })).not.toBeInTheDocument()

    seedPane('left', { entries: folderEntries(501) })
    expect(screen.getByRole('button', { name: calculateName })).toBeInTheDocument()

    seedPane('left', { entries: [] })
    act(() => useConfigStore.setState({ autoFolderSize: false }))
    expect(screen.getByRole('button', { name: calculateName })).toBeInTheDocument()
  })

  it('shows the size action when automatic sizing left a folder unknown', () => {
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'macIndex', isAvailable: true, state: 'upToDate' },
    })
    const entries = folderEntries(2)
    seedPane('left', { entries })
    renderBar()
    expect(screen.queryByRole('button', { name: calculateName })).not.toBeInTheDocument()

    act(() =>
      usePanesStore
        .getState()
        .applySizeStates([
          { path: entries[1].path, state: 'unknown', source: 'index', sizeBytes: null },
        ]),
    )
    expect(screen.getByRole('button', { name: calculateName })).toBeInTheDocument()
  })

  it('toggles hidden files with a pressed state', async () => {
    const user = userEvent.setup()
    const setShowHiddenFiles = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ setShowHiddenFiles, showHiddenFiles: true })
    renderBar()

    const hidden = screen.getByRole('button', { name: 'Hidden files' })
    expect(hidden).toHaveAttribute('aria-pressed', 'true')
    expect(hidden).toHaveAttribute('title', 'Hidden files (on)')
    await user.click(hidden)

    expect(setShowHiddenFiles).toHaveBeenCalledWith(false)
  })

  it('opens settings and flips the theme', async () => {
    const user = userEvent.setup()
    const setTheme = vi.fn()
    const view = renderBar('light', setTheme)

    await user.click(screen.getByRole('button', { name: 'Settings' }))
    expect(useSettingsStore.getState().isOpen).toBe(true)

    await user.click(screen.getByRole('button', { name: 'Dark theme' }))
    expect(setTheme).toHaveBeenCalledWith('dark')

    view.unmount()
    renderBar('dark', setTheme)
    await user.click(screen.getByRole('button', { name: 'Light theme' }))
    expect(setTheme).toHaveBeenLastCalledWith('light')
  })

  it("binds the one filter to the active pane's own text and folder name", async () => {
    const user = userEvent.setup()
    seedPane('left', { path: 'C:\\Users\\Omega', filterDraft: 'doc' })
    seedPane('right', { path: 'D:\\Projects\\src', filterDraft: '' })
    renderBar()

    const leftFilter = screen.getByRole('textbox', { name: 'Left pane filter' })
    expect(leftFilter).toHaveValue('doc')
    expect(leftFilter).toHaveAttribute('placeholder', 'Filter Omega')

    act(() => usePanesStore.getState().setActivePane('right'))
    const rightFilter = screen.getByRole('textbox', { name: 'Right pane filter' })
    expect(rightFilter).toHaveValue('')
    expect(rightFilter).toHaveAttribute('placeholder', 'Filter src')

    await user.type(rightFilter, 'a')
    expect(usePanesStore.getState().panes.right.filterDraft).toBe('a')
    // The other pane keeps its own filter text.
    expect(usePanesStore.getState().panes.left.filterDraft).toBe('doc')
  })

  it('forwards list keys to the active pane and clears on Escape', async () => {
    const user = userEvent.setup()
    const leftKeys = vi.fn()
    const rightKeys = vi.fn()
    const unregisterLeft = registerPaneFilterKeys('left', (event) => leftKeys(event.key))
    const unregisterRight = registerPaneFilterKeys('right', (event) => rightKeys(event.key))
    const clearFilter = vi.fn()
    usePanesStore.setState({ clearFilter })
    try {
      renderBar()
      const filter = screen.getByRole('textbox', { name: 'Left pane filter' })
      filter.focus()

      await user.keyboard('{ArrowDown}')
      expect(leftKeys).toHaveBeenCalledWith('ArrowDown')
      expect(rightKeys).not.toHaveBeenCalled()

      await user.keyboard('{Escape}')
      expect(clearFilter).toHaveBeenCalledWith('left')
      expect(document.activeElement).toHaveAttribute('data-pane-id', 'left')
      expect(leftKeys).not.toHaveBeenCalledWith('Escape')
    } finally {
      unregisterLeft()
      unregisterRight()
    }
  })
})
