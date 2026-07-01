import { beforeEach, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { runCompressCommand, runExtractCommand } from '@/lib/archive-commands'
import { ipc } from '@/tests/ipc-mock'
import { canExecuteCommand, executeCommand, selectedEntriesForPane } from '@/lib/commands'
import { showPropertiesDialog, toPropertiesDialogItem } from '@/lib/properties-commands'
import { TRASH_PATH } from '@/lib/trash'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useErrorToastStore } from '@/stores/error-toast-store'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { usePanesStore } from '@/stores/panes-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useSettingsStore } from '@/stores/settings-store'
import type { DirectoryEntry } from '@/lib/types/ipc'

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

function trashEntry(name: string, hasOriginalPath = true): DirectoryEntry {
  return {
    ...entry(name, false),
    trashId: name,
    originalPath: hasOriginalPath ? `C:\\root\\${name}` : undefined,
  }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useSelectionStore.getState().reset()
  useActionDialogStore.getState().close()
  useClipboardStore.getState().clearClipboard()
  useInlineRenameStore.getState().reset()
  usePropertiesDialogStore.getState().close()
  useSettingsStore.getState().close()
  usePanesStore.setState((state) => ({
    panes: {
      ...state.panes,
      left: {
        ...state.panes.left,
        path: 'C:\\root',
        entries: [entry('Alpha'), entry('Beta', false)],
      },
      right: {
        ...state.panes.right,
        path: 'D:\\dest',
      },
    },
  }))
})

describe('executeCommand file actions', () => {
  it('opens the new-folder prompt', () => {
    executeCommand('newFolder', 'left')
    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'newFolder',
      paneId: 'left',
    })
  })

  it('opens the new-file prompt', () => {
    executeCommand('newFile', 'left')
    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'newFile',
      paneId: 'left',
    })
  })

  it('opens non-folder items through the OS opener command', async () => {
    const openPath = vi.fn(() => undefined)
    ipc.override('open_path', openPath)

    executeCommand('open', 'left', 'Beta')

    expect(openPath).toHaveBeenCalledWith({ path: 'C:\\root\\Beta' })
  })

  it('navigates folders and parent rows through the pane store', () => {
    const navigatePane = vi.fn()
    const goUp = vi.fn()
    usePanesStore.setState({ navigatePane, goUp })

    executeCommand('open', 'left', 'Alpha')
    executeCommand('open', 'left', '..')

    expect(navigatePane).toHaveBeenCalledWith('left', 'C:\\root\\Alpha')
    expect(goUp).toHaveBeenCalledWith('left')
  })

  it('runs pane-level commands for go-up, refresh, clear-filter, select-all, and settings', () => {
    const goUp = vi.fn()
    const refreshEverything = vi.fn()
    const clearFilter = vi.fn()
    usePanesStore.setState({ goUp, refreshEverything, clearFilter })

    // showSettings reopens the section the user last viewed this session.
    useSettingsStore.setState({ section: 'columns' })

    executeCommand('goUp', 'left')
    executeCommand('refresh', 'left')
    executeCommand('clearFilter', 'left')
    executeCommand('selectAll', 'left')
    executeCommand('showSettings', 'left')

    expect(goUp).toHaveBeenCalledWith('left')
    expect(refreshEverything).toHaveBeenCalledWith('left')
    expect(clearFilter).toHaveBeenCalledWith('left')
    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual(['Alpha', 'Beta'])
    expect(useSettingsStore.getState().isOpen).toBe(true)
    expect(useSettingsStore.getState().section).toBe('columns')
  })

  it('requests manual size only for the focused directory target', () => {
    const requestManualSize = vi.fn()
    usePanesStore.setState({ requestManualSize })

    executeCommand('calculateSize', 'left', 'Alpha')
    executeCommand('calculateSize', 'left')

    expect(requestManualSize).toHaveBeenCalledWith('left', 'Alpha')
    expect(requestManualSize).toHaveBeenCalledOnce()
  })

  it('opens directories in a new tab or the opposite pane', () => {
    const openTabFromPath = vi.fn()
    const navigatePane = vi.fn()
    usePanesStore.setState({ openTabFromPath, navigatePane })

    executeCommand('openInNewTab', 'left', 'Alpha')
    executeCommand('openInNewTab', 'left', 'Beta')
    executeCommand('openInOtherPane', 'left', 'Alpha')
    executeCommand('openInOtherPane', 'left', 'Beta')

    expect(openTabFromPath).toHaveBeenCalledOnce()
    expect(openTabFromPath).toHaveBeenCalledWith('left', 'C:\\root\\Alpha')
    expect(navigatePane).toHaveBeenCalledOnce()
    expect(navigatePane).toHaveBeenCalledWith('right', 'C:\\root\\Alpha')
  })

  it('starts an inline rename seeded with the target name', () => {
    executeCommand('rename', 'left', 'Alpha')
    expect(useInlineRenameStore.getState().rename).toMatchObject({
      paneId: 'left',
      entryId: 'Alpha',
      initialValue: 'Alpha',
      path: 'C:\\root\\Alpha',
    })
  })

  it('renames the sole selected entry when invoked without an explicit target', () => {
    useSelectionStore.getState().setSelection('left', ['Beta'], 'Beta', 'Beta')
    executeCommand('rename', 'left')
    expect(useInlineRenameStore.getState().rename).toMatchObject({ entryId: 'Beta' })
  })

  it('does not start inline rename when multiple entries are selected', () => {
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Beta'], 'Alpha', 'Beta')
    executeCommand('rename', 'left')
    expect(useInlineRenameStore.getState().rename).toBeNull()
  })

  it('moves selected entries to trash without opening a dialog', async () => {
    const moveToTrash = vi.fn(() => undefined)
    ipc.override('move_to_trash', moveToTrash)
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Beta'], 'Alpha', 'Beta')

    executeCommand('delete', 'left')

    await waitFor(() => expect(moveToTrash).toHaveBeenCalledOnce())
    expect(moveToTrash).toHaveBeenCalledWith({
      paths: ['C:\\root\\Alpha', 'C:\\root\\Beta'],
    })
    expect(useActionDialogStore.getState().dialog).toBeNull()
  })

  it('opens a permanent delete confirmation for the selected entries', () => {
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Beta'], 'Alpha', 'Beta')
    executeCommand('deletePermanent', 'left')
    const dialog = useActionDialogStore.getState().dialog
    expect(dialog?.kind).toBe('delete')
    expect(dialog).toMatchObject({
      targets: [
        { id: 'Alpha', path: 'C:\\root\\Alpha' },
        { id: 'Beta', path: 'C:\\root\\Beta' },
      ],
    })
  })

  it('copies and cuts selected entries into the clipboard', () => {
    const writeFileClipboard = vi.fn(() => undefined)
    ipc.override('write_file_clipboard', writeFileClipboard)
    useSelectionStore.getState().setSelection('left', ['Beta'], 'Beta', 'Beta')

    executeCommand('copy', 'left')
    expect(useClipboardStore.getState()).toMatchObject({
      mode: 'copy',
      sourcePaneId: 'left',
      entries: [{ id: 'Beta' }],
    })
    expect(writeFileClipboard).toHaveBeenNthCalledWith(1, {
      mode: 'copy',
      paths: ['C:\\root\\Beta'],
    })

    executeCommand('cut', 'left', 'Alpha')
    expect(useClipboardStore.getState()).toMatchObject({
      mode: 'move',
      sourcePaneId: 'left',
      entries: [{ id: 'Alpha' }],
    })
    expect(writeFileClipboard).toHaveBeenNthCalledWith(2, {
      mode: 'move',
      paths: ['C:\\root\\Alpha'],
    })
  })

  it('pastes clipboard entries through the queue and clears moved items after enqueue', async () => {
    const startOp = vi.fn(() => 'op-1')
    const clearFileClipboard = vi.fn(() => undefined)
    ipc.override('start_op', startOp)
    ipc.override('clear_file_clipboard', clearFileClipboard)
    useClipboardStore.getState().setClipboard('move', 'right', [entry('Beta', false)])

    executeCommand('paste', 'left')
    await waitFor(() => expect(startOp).toHaveBeenCalledOnce())

    expect(startOp).toHaveBeenCalledWith({
      kind: 'move',
      destinationDir: 'C:\\root',
      items: [{ sourcePath: 'C:\\root\\Beta', name: 'Beta', sizeBytes: 10 }],
    })
    expect(useClipboardStore.getState().entries).toEqual([])
    expect(clearFileClipboard).toHaveBeenCalledOnce()
  })

  it('does not paste with an empty clipboard', () => {
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)

    executeCommand('paste', 'left')

    expect(startOp).not.toHaveBeenCalled()
  })

  it('opens a transfer confirmation for copy-to-other-pane with source and destination folders', () => {
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        right: { ...state.panes.right, path: 'D:\\dest' },
      },
    }))
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Beta'], 'Alpha', 'Beta')

    executeCommand('copyToOtherPane', 'left')

    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'transferConfirm',
      operation: 'copy',
      sourceDir: 'C:\\root',
      destinationDir: 'D:\\dest',
      targets: [
        { id: 'Alpha', path: 'C:\\root\\Alpha' },
        { id: 'Beta', path: 'C:\\root\\Beta' },
      ],
    })
  })

  it('uses the focused item as the transfer target when there is no explicit target or selection', () => {
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: { ...state.panes.left, focusedEntryId: 'Alpha' },
      },
    }))

    executeCommand('moveToOtherPane', 'left')

    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'transferConfirm',
      operation: 'move',
      targets: [{ id: 'Alpha', path: 'C:\\root\\Alpha' }],
    })
  })

  it('gates rename/delete on a target or selection', () => {
    expect(canExecuteCommand('rename', 'left', 'Alpha')).toBe(true)
    expect(canExecuteCommand('delete', 'left')).toBe(false)
    expect(canExecuteCommand('deletePermanent', 'left')).toBe(false)
    useSelectionStore.getState().setSelection('left', ['Alpha'], 'Alpha', 'Alpha')
    expect(canExecuteCommand('delete', 'left')).toBe(true)
    expect(canExecuteCommand('deletePermanent', 'left')).toBe(true)
  })

  it('gates commands that require clipboard entries, directory targets, or transfer candidates', () => {
    expect(canExecuteCommand('paste', 'left')).toBe(false)
    useClipboardStore.getState().setClipboard('copy', 'left', [entry('Beta', false)])
    expect(canExecuteCommand('paste', 'left')).toBe(true)

    expect(canExecuteCommand('selectAll', 'left')).toBe(true)
    expect(canExecuteCommand('openInNewTab', 'left', 'Alpha')).toBe(true)
    expect(canExecuteCommand('openInOtherPane', 'left', 'Beta')).toBe(false)
    expect(canExecuteCommand('calculateSize', 'left', 'Alpha')).toBe(true)
    expect(canExecuteCommand('copyToOtherPane', 'left')).toBe(false)

    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: { ...state.panes.left, focusedEntryId: 'Beta' },
      },
    }))
    expect(canExecuteCommand('moveToOtherPane', 'left')).toBe(true)
    expect(canExecuteCommand('showSettings', 'left')).toBe(true)
  })

  it('returns the concrete selected entries for the pane', () => {
    useSelectionStore.getState().setSelection('left', ['Beta'], 'Beta', 'Beta')

    expect(selectedEntriesForPane('left')).toEqual([
      expect.objectContaining({
        id: 'Beta',
        path: 'C:\\root\\Beta',
      }),
    ])
  })
})

describe('archive and properties helpers', () => {
  it('returns handled archive responses through the centralized IPC wrapper', async () => {
    ipc.override('compress_archive', () => ({ handled: true, message: 'C:\\root\\Alpha.zip' }))
    ipc.override('extract_archive', () => ({ handled: true, message: 'C:\\root\\Alpha' }))

    await expect(
      runCompressCommand({
        paths: ['C:\\root\\Alpha'],
        destinationDir: 'C:\\root',
      }),
    ).resolves.toMatchObject({ handled: true, message: 'C:\\root\\Alpha.zip' })

    await expect(
      runExtractCommand({
        paths: ['C:\\root\\Alpha.zip'],
        destinationDir: 'C:\\root',
      }),
    ).resolves.toMatchObject({ handled: true, message: 'C:\\root\\Alpha' })
  })

  it('opens the fallback properties dialog when native properties are unsupported', async () => {
    ipc.override('show_properties', () => ({ handled: false, message: 'unsupported' }))

    await showPropertiesDialog([toPropertiesDialogItem(entry('Beta', false))])

    expect(usePropertiesDialogStore.getState().dialog).toMatchObject({
      items: [expect.objectContaining({ path: 'C:\\root\\Beta' })],
    })
  })

  it('attempts native properties for multi-selection and falls back when unsupported', async () => {
    const nativeProperties = vi.fn(() => ({ handled: true, message: 'properties-opened' }))
    ipc.override('show_properties', nativeProperties)

    await showPropertiesDialog([
      toPropertiesDialogItem(entry('Alpha')),
      toPropertiesDialogItem(entry('Beta', false)),
    ])

    expect(nativeProperties).toHaveBeenCalledWith({
      paths: ['C:\\root\\Alpha', 'C:\\root\\Beta'],
    })
    expect(usePropertiesDialogStore.getState().dialog).toBeNull()
  })
})

describe('trash pane commands', () => {
  beforeEach(() => {
    act(() => {
      useErrorToastStore.getState().dismiss()
    })
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: TRASH_PATH,
          entries: [trashEntry('report.txt'), trashEntry('orphan.txt', false)],
        },
      },
    }))
  })

  it('is a no-op outside the trash pane and gates restore/emptyTrash there', () => {
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, left: { ...state.panes.left, path: 'C:\\root' } },
    }))
    expect(canExecuteCommand('restore', 'left', 'report.txt')).toBe(false)
    expect(canExecuteCommand('emptyTrash', 'left')).toBe(false)
  })

  it('requires a known original location to allow restore', () => {
    expect(canExecuteCommand('restore', 'left', 'report.txt')).toBe(true)
    expect(canExecuteCommand('restore', 'left', 'orphan.txt')).toBe(false)
    expect(canExecuteCommand('emptyTrash', 'left')).toBe(true)
  })

  it('restores the target entry and reloads the pane', async () => {
    const restoreFromTrash = vi.fn(() => undefined)
    ipc.override('restore_from_trash', restoreFromTrash)
    ipc.override('list_trash', () => ({ entries: [] }))

    executeCommand('restore', 'left', 'report.txt')

    await waitFor(() => expect(restoreFromTrash).toHaveBeenCalledWith({ ids: ['report.txt'] }))
    await waitFor(() => expect(usePanesStore.getState().panes.left.entries).toEqual([]))
  })

  it('shows an error toast when restore fails', async () => {
    ipc.override('restore_from_trash', () =>
      Promise.reject(
        new Error("'report.txt' has no known original location and cannot be restored"),
      ) as never,
    )

    executeCommand('restore', 'left', 'report.txt')

    await waitFor(() =>
      expect(useErrorToastStore.getState().message).toBe(
        "'report.txt' has no known original location and cannot be restored",
      ),
    )
  })

  it('opens a delete-from-trash confirmation for plain delete and delete-permanent alike', () => {
    executeCommand('delete', 'left', 'report.txt')
    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'deleteFromTrash',
      targets: [{ id: 'report.txt', name: 'report.txt' }],
    })

    useActionDialogStore.getState().close()
    executeCommand('deletePermanent', 'left', 'orphan.txt')
    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'deleteFromTrash',
      targets: [{ id: 'orphan.txt', name: 'orphan.txt' }],
    })
  })

  it('does not require an original location to permanently delete a trash entry', () => {
    expect(canExecuteCommand('delete', 'left', 'orphan.txt')).toBe(true)
    expect(canExecuteCommand('deletePermanent', 'left', 'orphan.txt')).toBe(true)
  })
})
