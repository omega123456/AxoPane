import { beforeEach } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { canExecuteCommand, executeCommand } from '@/lib/commands'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
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

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useSelectionStore.getState().reset()
  useActionDialogStore.getState().close()
  useInlineRenameStore.getState().reset()
  usePanesStore.setState((state) => ({
    panes: {
      ...state.panes,
      left: { ...state.panes.left, path: 'C:\\root', entries: [entry('Alpha'), entry('Beta', false)] },
    },
  }))
})

describe('executeCommand file actions', () => {
  it('opens the new-folder prompt', () => {
    executeCommand('newFolder', 'left')
    expect(useActionDialogStore.getState().dialog).toMatchObject({ kind: 'newFolder', paneId: 'left' })
  })

  it('opens the new-file prompt', () => {
    executeCommand('newFile', 'left')
    expect(useActionDialogStore.getState().dialog).toMatchObject({ kind: 'newFile', paneId: 'left' })
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

  it('opens a delete confirmation for the selected entries', () => {
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Beta'], 'Alpha', 'Beta')
    executeCommand('delete', 'left')
    const dialog = useActionDialogStore.getState().dialog
    expect(dialog?.kind).toBe('delete')
    expect(dialog).toMatchObject({
      targets: [
        { id: 'Alpha', path: 'C:\\root\\Alpha' },
        { id: 'Beta', path: 'C:\\root\\Beta' },
      ],
    })
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

  it('gates rename/delete on a target or selection', () => {
    expect(canExecuteCommand('rename', 'left', 'Alpha')).toBe(true)
    expect(canExecuteCommand('delete', 'left')).toBe(false)
    useSelectionStore.getState().setSelection('left', ['Alpha'], 'Alpha', 'Alpha')
    expect(canExecuteCommand('delete', 'left')).toBe(true)
  })
})
