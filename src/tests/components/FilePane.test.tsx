import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { ActionDialog } from '@/components/dialogs/ActionDialog'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { FilePane } from '@/components/pane/FilePane'
import { executeCommand } from '@/lib/commands'
import { resolveCommandForEvent } from '@/lib/keymap'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { DirectoryEntry, LoadNativeMenuRequest } from '@/lib/types/ipc'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

function entry(name: string, isDir = true): DirectoryEntry {
  return {
    id: name,
    name,
    path: `C:\\root\\${name}`,
    isDir,
    sizeBytes: isDir ? null : 10,
    itemCount: isDir ? 1 : null,
    typeLabel: isDir ? 'Folder' : 'File',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

function seedPane(partial: Partial<ReturnType<typeof usePanesStore.getState>['panes']['left']>) {
  usePanesStore.setState((state) => ({
    panes: {
      ...state.panes,
      left: { ...state.panes.left, path: 'C:\\root\\dir', ...partial },
    },
  }))
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useSelectionStore.getState().reset()
  useInlineRenameStore.getState().reset()
  useActionDialogStore.getState().close()
})

afterEach(() => {
  setPlatform(originalPlatform)
})

describe('FilePane state rendering', () => {
  it('renders the loading skeleton only after loading persists past the delay', () => {
    vi.useFakeTimers()
    try {
      seedPane({ loading: true })
      render(<FilePane paneId="left" />)

      // Suppressed initially to avoid a flash on fast loads.
      expect(screen.queryByRole('status', { name: 'Loading folder' })).not.toBeInTheDocument()

      act(() => {
        vi.advanceTimersByTime(1000)
      })

      expect(screen.getByRole('status', { name: 'Loading folder' })).toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not flash the loading skeleton when a load resolves quickly', () => {
    vi.useFakeTimers()
    try {
      seedPane({ loading: true })
      const view = render(<FilePane paneId="left" />)

      act(() => {
        vi.advanceTimersByTime(200)
      })
      // Loading finishes before the delay elapses.
      act(() => {
        seedPane({ loading: false })
        view.rerender(<FilePane paneId="left" />)
      })

      act(() => {
        vi.advanceTimersByTime(1000)
      })

      expect(screen.queryByRole('status', { name: 'Loading folder' })).not.toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })

  it('renders the empty state at a root with no parent', () => {
    seedPane({ path: 'C:\\', entries: [] })
    render(<FilePane paneId="left" />)
    expect(screen.getByText('This folder is empty')).toBeInTheDocument()
    expect(screen.queryByRole('row', { name: 'Go to parent folder' })).not.toBeInTheDocument()
  })

  it('renders a synthetic parent row that navigates up on activation', async () => {
    const user = userEvent.setup()
    const goUp = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ goUp })
    seedPane({ path: 'C:\\root\\dir', entries: [] })

    render(<FilePane paneId="left" />)
    const parentRow = screen.getByRole('row', { name: 'Go to parent folder' })
    expect(parentRow).toBeInTheDocument()
    expect(parentRow).toHaveTextContent('..')

    await user.dblClick(parentRow)
    expect(goUp).toHaveBeenCalledWith('left')
  })

  it('focuses the parent row on single click and activates it with Enter', async () => {
    const user = userEvent.setup()
    const goUp = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ goUp })
    seedPane({ path: 'C:\\root\\dir', entries: [entry('Alpha')], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)
    const pane = screen.getByLabelText('Left pane')
    const parentRow = screen.getByRole('row', { name: 'Go to parent folder' })

    await user.click(parentRow)
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('..')

    pane.focus()
    await user.keyboard('{Enter}')
    expect(goUp).toHaveBeenCalledWith('left')
  })

  it('ignores an immediate follow-up parent-row double-click after the pane path changes', () => {
    const originalGoUp = usePanesStore.getState().goUp
    const goUp = vi.fn(() => Promise.resolve())
    try {
      usePanesStore.setState({ goUp })
      seedPane({ path: 'C:\\root\\dir', entries: [] })

      const view = render(<FilePane paneId="left" />)
      fireEvent.doubleClick(screen.getByRole('row', { name: 'Go to parent folder' }))

      act(() => {
        usePanesStore.setState((state) => ({
          panes: {
            ...state.panes,
            left: { ...state.panes.left, path: 'C:\\root', entries: [] },
          },
        }))
      })
      view.rerender(<FilePane paneId="left" />)

      fireEvent.doubleClick(screen.getByRole('row', { name: 'Go to parent folder' }))
      expect(goUp).toHaveBeenCalledOnce()
    } finally {
      act(() => {
        usePanesStore.setState({ goUp: originalGoUp })
      })
    }
  })

  it('omits the parent row at a drive root', () => {
    seedPane({ path: 'C:\\', entries: [entry('Alpha')] })
    render(<FilePane paneId="left" />)
    expect(screen.queryByRole('row', { name: 'Go to parent folder' })).not.toBeInTheDocument()
  })

  it('focuses the parent row when arrowing up from the first entry', async () => {
    const user = userEvent.setup()
    seedPane({
      path: 'C:\\root\\dir',
      entries: [entry('Alpha'), entry('Beta')],
      focusedEntryId: 'Alpha',
    })

    render(<FilePane paneId="left" />)
    const pane = screen.getByLabelText('Left pane')
    pane.focus()

    await user.keyboard('{ArrowUp}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('..')

    await user.keyboard('{ArrowDown}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('Alpha')
  })

  it('renders the error state', () => {
    seedPane({ error: 'Something broke' })
    render(<FilePane paneId="left" />)
    expect(screen.getByRole('alert')).toHaveTextContent('Something broke')
  })

  it('renders permission denied for an access error', () => {
    seedPane({ error: 'Access is denied' })
    render(<FilePane paneId="left" />)
    expect(screen.getByRole('alert', { name: 'Permission denied' })).toBeInTheDocument()
  })

  it('opens a folder in a new tab via middle-click', async () => {
    const user = userEvent.setup()
    ipc.override('list_dir', (payload) => ({ path: payload.path, entries: [] }))
    ipc.override('set_tab_watch', () => undefined)
    ipc.override('save_session', (payload) => payload.session)
    seedPane({ entries: [entry('Alpha')], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)
    const row = within(screen.getByLabelText('Left pane'))
      .getAllByRole('row')
      .find((node) => node.textContent?.includes('Alpha'))
    if (!row) {
      throw new Error('row missing')
    }

    await user.pointer({ keys: '[MouseMiddle]', target: row })
    expect(useTabsStore.getState().panes.left.tabs.length).toBeGreaterThan(1)
  })

  it('ignores an immediate follow-up folder double-click after a fast path change', () => {
    const originalNavigatePane = usePanesStore.getState().navigatePane
    const navigatePane = vi.fn(() => Promise.resolve())
    try {
      usePanesStore.setState({ navigatePane })
      seedPane({
        entries: [entry('Alpha')],
        focusedEntryId: 'Alpha',
      })

      const view = render(<FilePane paneId="left" />)
      fireEvent.doubleClick(screen.getByRole('row', { name: /Alpha/ }))

      act(() => {
        usePanesStore.setState((state) => ({
          panes: {
            ...state.panes,
            left: {
              ...state.panes.left,
              path: 'C:\\root\\Alpha',
              entries: [entry('Alpha')],
              focusedEntryId: 'Alpha',
            },
          },
        }))
      })
      view.rerender(<FilePane paneId="left" />)

      fireEvent.doubleClick(screen.getByRole('row', { name: /Alpha/ }))
      expect(navigatePane).toHaveBeenCalledOnce()
      expect(navigatePane).toHaveBeenCalledWith('left', 'C:\\root\\Alpha')
    } finally {
      act(() => {
        usePanesStore.setState({ navigatePane: originalNavigatePane })
      })
    }
  })

  it('restores history scroll after returning from a folder too short to scroll', () => {
    const longEntries = Array.from({ length: 40 }, (_, index) => entry(`Item ${index}`))
    seedPane({
      path: 'C:\\root',
      entries: longEntries,
      scrollPositions: { 'C:\\root': 180 },
    })

    const view = render(<FilePane paneId="left" />)
    const scroller = screen.getByLabelText('Left pane').querySelector<HTMLElement>('.overflow-auto')
    if (!scroller) {
      throw new Error('scroll container missing')
    }

    let canScroll = true
    let scrollTop = 180
    Object.defineProperty(scroller, 'scrollTop', {
      configurable: true,
      get: () => scrollTop,
      set: (value: number) => {
        scrollTop = canScroll ? value : 0
      },
    })

    act(() => {
      canScroll = false
      usePanesStore.setState((state) => ({
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            path: 'C:\\root\\short',
            entries: [entry('Only child')],
            scrollPositions: { 'C:\\root': 180, 'C:\\root\\short': 0 },
          },
        },
      }))
      view.rerender(<FilePane paneId="left" />)
    })
    expect(scroller.scrollTop).toBe(0)

    act(() => {
      usePanesStore.setState((state) => ({
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            path: 'C:\\root',
            entries: [entry('Only child')],
            scrollPositions: { 'C:\\root': 180, 'C:\\root\\short': 0 },
          },
        },
      }))
      view.rerender(<FilePane paneId="left" />)
    })
    expect(scroller.scrollTop).toBe(0)

    act(() => {
      canScroll = true
      usePanesStore.setState((state) => ({
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            entries: longEntries,
          },
        },
      }))
      view.rerender(<FilePane paneId="left" />)
    })

    expect(scroller.scrollTop).toBe(180)
  })

  it('opens non-folder items with the OS default application on activation', async () => {
    const user = userEvent.setup()
    const openPath = vi.fn(() => undefined)
    ipc.override('open_path', openPath)
    seedPane({ entries: [entry('Report.txt', false)], focusedEntryId: 'Report.txt' })

    render(<FilePane paneId="left" />)
    const row = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Report\.txt/ })

    await user.dblClick(row)
    expect(openPath).toHaveBeenCalledWith({ path: 'C:\\root\\Report.txt' })
  })

  it('handles arrow navigation, Ctrl+R refresh, and Backspace', async () => {
    const user = userEvent.setup()
    const refreshEverything = vi.fn(() => Promise.resolve())
    const goUp = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ refreshEverything, goUp })
    seedPane({ entries: [entry('Alpha'), entry('Beta')], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)
    const pane = screen.getByLabelText('Left pane')
    pane.focus()

    await user.keyboard('{ArrowDown}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('Beta')
    await user.keyboard('{ArrowUp}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('Alpha')

    await user.keyboard('{Control>}r{/Control}')
    expect(refreshEverything).toHaveBeenCalledWith('left')

    await user.keyboard('{Backspace}')
    expect(goUp).toHaveBeenCalledWith('left')
  })

  it('lets the filter input keep Backspace instead of triggering go up', async () => {
    const user = userEvent.setup()
    const goUp = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ goUp })
    seedPane({ entries: [entry('Alpha')], filterDraft: 'Media', filterApplied: 'Media' })

    render(<FilePane paneId="left" />)
    const filter = screen.getByRole('textbox', { name: 'Left pane filter' })

    await user.click(filter)
    await user.keyboard('{Backspace}')

    expect(filter).toHaveValue('Medi')
    expect(goUp).not.toHaveBeenCalled()

    await waitFor(() => {
      expect(usePanesStore.getState().panes.left.filterApplied).toBe('Medi')
    })
  })

  it('navigates and opens the filtered list while keeping focus in the filter input', async () => {
    const user = userEvent.setup()
    ipc.override('list_dir', (payload) => {
      if (payload.path === 'C:\\root\\Beta') {
        return { path: payload.path, entries: [entry('Nested')] }
      }

      return { path: payload.path, entries: [entry('Alpha'), entry('Beta')] }
    })
    ipc.override('set_tab_watch', () => undefined)
    ipc.override('save_session', (payload) => payload.session)
    seedPane({
      path: 'C:\\root',
      entries: [entry('Alpha'), entry('Beta')],
      focusedEntryId: 'Alpha',
    })

    render(<FilePane paneId="left" />)
    const filter = screen.getByRole('textbox', { name: 'Left pane filter' })
    await user.click(filter)

    await user.keyboard('{ArrowDown}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('Beta')
    // Arrow navigation must not steal focus away from the filter input.
    expect(document.activeElement).toBe(filter)

    await user.keyboard('{ArrowUp}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('Alpha')
    expect(document.activeElement).toBe(filter)

    await user.keyboard('{ArrowDown}')
    await user.keyboard('{Enter}')
    await waitFor(() => {
      expect(usePanesStore.getState().panes.left.path).toBe('C:\\root\\Beta')
    })
    // Opening the folder returns focus to the pane shell so arrow keys keep working.
    expect(document.activeElement).toBe(screen.getByLabelText('Left pane'))
  })

  it('clears the filter and returns focus to the pane on Escape', async () => {
    const user = userEvent.setup()
    // Clearing the filter reloads the folder; pin the result so the post-clear
    // list is deterministic.
    ipc.override('list_dir', (payload) => ({ path: payload.path, entries: [entry('Alpha')] }))
    seedPane({
      path: 'C:\\root\\dir',
      entries: [entry('Alpha')],
      filterDraft: 'Media',
      filterApplied: 'Media',
    })

    render(<FilePane paneId="left" />)
    const pane = screen.getByLabelText('Left pane')
    const filter = screen.getByRole('textbox', { name: 'Left pane filter' })

    await user.click(filter)
    await user.keyboard('{Escape}')

    expect(filter).toHaveValue('')
    // Focus is back on the pane shell, so arrow keys resume driving the list.
    expect(document.activeElement).toBe(pane)
  })

  it('still types ordinary characters into the focused filter input', async () => {
    const user = userEvent.setup()
    ipc.override('list_dir', (payload) => ({ path: payload.path, entries: [entry('Alpha')] }))
    seedPane({ entries: [entry('Alpha')], filterDraft: 'Me', filterApplied: 'Me' })

    render(<FilePane paneId="left" />)
    const filter = screen.getByRole('textbox', { name: 'Left pane filter' })

    await user.click(filter)
    await user.keyboard('dia')

    expect(filter).toHaveValue('Media')
    // Let the debounced filter settle so its reload timer doesn't leak into the
    // next test.
    await waitFor(() => {
      expect(usePanesStore.getState().panes.left.filterApplied).toBe('Media')
    })
  })

  it('renames inline instead of opening a modal', async () => {
    const rename = vi.fn((payload: { path: string; newName: string }) => ({
      ...entry(payload.newName, false),
      path: `C:\\root\\${payload.newName}`,
    }))
    ipc.override('rename_entry', rename)
    ipc.override('list_dir', (payload) => ({
      path: payload.path,
      entries: [entry('Reports', false)],
    }))
    ipc.override('set_tab_watch', () => undefined)
    ipc.override('save_session', (payload) => payload.session)
    seedPane({ path: 'C:\\root', entries: [entry('Alpha', false)], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)
    act(() => {
      executeCommand('rename', 'left', 'Alpha')
    })

    const input = await screen.findByRole('textbox', { name: 'Rename Alpha' })
    expect(screen.queryByRole('dialog', { name: 'Rename' })).not.toBeInTheDocument()

    fireEvent.change(input, { target: { value: 'Reports' } })
    fireEvent.keyDown(input, { key: 'Enter' })

    await waitFor(() => {
      expect(rename).toHaveBeenCalledWith({ path: 'C:\\root\\Alpha', newName: 'Reports' })
    })
    await waitFor(() => {
      expect(screen.queryByRole('textbox', { name: 'Rename Alpha' })).not.toBeInTheDocument()
    })
  })

  it('keeps pane keyboard focus after opening a folder with the mouse', async () => {
    const user = userEvent.setup()
    ipc.override('list_dir', (payload) => {
      if (payload.path === 'C:\\root\\Alpha') {
        return { path: payload.path, entries: [entry('Nested A'), entry('Nested B')] }
      }

      return { path: payload.path, entries: [entry('Alpha')] }
    })
    ipc.override('set_tab_watch', () => undefined)
    ipc.override('save_session', (payload) => payload.session)
    seedPane({ path: 'C:\\root', entries: [entry('Alpha')], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)
    const pane = screen.getByLabelText('Left pane')
    const row = within(pane).getByRole('row', { name: /Alpha/ })

    await user.dblClick(row)
    await screen.findByRole('row', { name: /Nested A/ })

    expect(document.activeElement).toBe(pane)

    await user.keyboard('{ArrowDown}')
    expect(usePanesStore.getState().panes.left.focusedEntryId).toBe('Nested B')
  })

  it('requires confirmation before cross-pane transfers from F5 and F6 start', async () => {
    const user = userEvent.setup()
    const startSpy = vi.fn(() => 'op-1')
    ipc.override('start_op', startSpy)
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\root',
          entries: [entry('Alpha', false)],
          focusedEntryId: 'Alpha',
        },
        right: {
          ...state.panes.right,
          path: 'D:\\dest',
        },
      },
    }))

    render(
      <>
        <FilePane paneId="left" />
        <ActionDialog />
      </>,
    )
    screen.getByLabelText('Left pane').focus()

    await user.keyboard('{F5}')
    const copyDialog = screen.getByRole('dialog', { name: 'Confirm copy' })
    expect(copyDialog).toBeInTheDocument()
    expect(within(copyDialog).getByText('C:\\root')).toBeInTheDocument()
    expect(within(copyDialog).getByText('D:\\dest')).toBeInTheDocument()
    expect(within(copyDialog).getByText('Alpha')).toBeInTheDocument()
    expect(startSpy).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Copy' }))

    expect(startSpy).toHaveBeenNthCalledWith(1, {
      kind: 'copy',
      destinationDir: 'D:\\dest',
      items: [{ sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 }],
    })

    screen.getByLabelText('Left pane').focus()
    await user.keyboard('{F6}')
    expect(screen.getByRole('dialog', { name: 'Confirm move' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Move' }))

    expect(startSpy).toHaveBeenNthCalledWith(2, {
      kind: 'move',
      destinationDir: 'D:\\dest',
      items: [{ sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 }],
    })
  })

  it('dispatches a focused-pane command only once when a global window fallback is also active', async () => {
    const user = userEvent.setup()
    const startSpy = vi.fn(() => 'op-1')
    ipc.override('start_op', startSpy)
    useClipboardStore.getState().setClipboard('copy', 'left', [entry('Alpha', false)])
    seedPane({ entries: [entry('Alpha', false)], focusedEntryId: 'Alpha' })

    // Mimic App.tsx's global window keydown fallback, which also resolves and
    // dispatches commands. The pane must stop propagation so this never fires a
    // second paste for one Ctrl+V (the duplicate-queue-entry regression).
    const fallback = (event: KeyboardEvent) => {
      const commandId = resolveCommandForEvent(event, useKeymapStore.getState().bindings)
      if (commandId) {
        executeCommand(commandId, 'left')
      }
    }
    window.addEventListener('keydown', fallback)

    try {
      render(<FilePane paneId="left" />)
      screen.getByLabelText('Left pane').focus()
      await user.keyboard('{Control>}v{/Control}')
      expect(startSpy).toHaveBeenCalledTimes(1)
    } finally {
      window.removeEventListener('keydown', fallback)
    }
  })

  it('selects every entry with Command+A on macOS', () => {
    setPlatform('MacIntel')
    seedPane({ entries: [entry('Alpha', false), entry('Beta', false)], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)
    const pane = screen.getByLabelText('Left pane')
    pane.focus()

    fireEvent.keyDown(pane, { key: 'a', metaKey: true })

    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual(['Alpha', 'Beta'])
  })

  it('suppresses pane shortcuts while an app-modal dialog is open', async () => {
    const user = userEvent.setup()
    const refreshEverything = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ refreshEverything })
    seedPane({ entries: [entry('Alpha')], focusedEntryId: 'Alpha' })
    useActionDialogStore.getState().open({
      kind: 'delete',
      paneId: 'left',
      targets: [{ id: 'Alpha', name: 'Alpha', path: 'C:\\root\\Alpha' }],
    })

    render(<FilePane paneId="left" />)
    screen.getByLabelText('Left pane').focus()
    await user.keyboard('{Control>}r{/Control}')

    expect(refreshEverything).not.toHaveBeenCalled()
  })

  it('dims rows whose paths are currently cut in the app clipboard', () => {
    useClipboardStore.getState().setClipboard('move', 'left', [entry('Alpha', false)])
    seedPane({ entries: [entry('Alpha', false), entry('Beta', false)], focusedEntryId: 'Alpha' })

    render(<FilePane paneId="left" />)

    const cutRow = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Alpha/ })
    const normalRow = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Beta/ })

    expect(cutRow).toHaveClass('opacity-50')
    expect(normalRow).not.toHaveClass('opacity-50')
  })

  it('requests native enrichment for the selected filesystem row without disturbing selection semantics', async () => {
    const user = userEvent.setup()
    const loadNativeMenu = vi.fn((payload: LoadNativeMenuRequest) => ({
      requestId: payload.requestId,
      items: [],
    }))
    ipc.override('load_native_menu', loadNativeMenu)
    seedPane({
      path: 'C:\\root',
      entries: [entry('Alpha', false), entry('Beta')],
      focusedEntryId: 'Beta',
    })
    useSelectionStore.getState().setSelection('left', ['Beta'], 'Beta', 'Beta')

    render(
      <>
        <FilePane paneId="left" />
        <ContextMenu />
      </>,
    )

    await user.pointer({
      keys: '[MouseRight]',
      target: within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Alpha/ }),
    })

    await waitFor(() => {
      expect(loadNativeMenu).toHaveBeenCalledTimes(1)
    })
    expect(loadNativeMenu).toHaveBeenCalledWith({
      requestId: expect.any(String),
      targetKind: 'file',
      targetPath: 'C:\\root\\Alpha',
      folderPath: 'C:\\root',
      selectedPaths: ['C:\\root\\Alpha'],
    })
    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual(['Alpha'])
  })
})
