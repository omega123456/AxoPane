import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { ActionDialog } from '@/components/dialogs/ActionDialog'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { usePanesStore } from '@/stores/panes-store'
import type { DirectoryEntry, ListDirRequest, ListDirResponse } from '@/lib/types/ipc'

function dir(name: string, isDir = true): DirectoryEntry {
  return {
    id: `C:\\root\\${name}`,
    name,
    path: `C:\\root\\${name}`,
    isDir,
    sizeBytes: isDir ? null : 10,
    itemCount: isDir ? 0 : null,
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
  useActionDialogStore.getState().close()
  ipc.override('list_dir', (payload: ListDirRequest): ListDirResponse => ({
    path: payload.path,
    entries: [dir('Reports')],
  }))
  ipc.override('set_tab_watch', () => undefined)
  ipc.override('save_session', (payload) => payload.session)
  usePanesStore.setState((state) => ({
    panes: { ...state.panes, left: { ...state.panes.left, path: 'C:\\root' } },
  }))
})

describe('ActionDialog', () => {
  it('creates a folder and closes on success', async () => {
    const user = userEvent.setup()
    const create = vi.fn((payload: { parent: string; name: string }) => dir(payload.name))
    ipc.override('create_folder', create)

    useActionDialogStore.getState().open({ kind: 'newFolder', paneId: 'left' })
    render(<ActionDialog />)

    await user.type(screen.getByLabelText('Folder name'), 'Reports')
    await user.click(screen.getByRole('button', { name: 'Create' }))

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(create).toHaveBeenCalledWith({ parent: 'C:\\root', name: 'Reports' })
  })

  it('shows the backend error and keeps the dialog open', async () => {
    const user = userEvent.setup()
    ipc.override('create_folder', () => {
      throw new Error('an item named "Reports" already exists')
    })

    useActionDialogStore.getState().open({ kind: 'newFolder', paneId: 'left' })
    render(<ActionDialog />)

    await user.type(screen.getByLabelText('Folder name'), 'Reports')
    await user.click(screen.getByRole('button', { name: 'Create' }))

    expect(await screen.findByText(/already exists/)).toBeInTheDocument()
    expect(useActionDialogStore.getState().dialog).not.toBeNull()
  })

  it('renames an entry from its current name', async () => {
    const user = userEvent.setup()
    const rename = vi.fn((payload: { path: string; newName: string }) => dir(payload.newName))
    ipc.override('rename_entry', rename)

    useActionDialogStore.getState().open({
      kind: 'rename',
      paneId: 'left',
      entryId: 'C:\\root\\Old',
      path: 'C:\\root\\Old',
      initialValue: 'Old',
    })
    render(<ActionDialog />)

    const input = screen.getByLabelText('New name')
    await user.clear(input)
    await user.type(input, 'Reports')
    await user.keyboard('{Enter}')

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(rename).toHaveBeenCalledWith({ path: 'C:\\root\\Old', newName: 'Reports' })
  })

  it('confirms a delete and calls the backend', async () => {
    const user = userEvent.setup()
    const remove = vi.fn(() => undefined)
    ipc.override('delete_entries', remove)

    useActionDialogStore.getState().open({
      kind: 'delete',
      paneId: 'left',
      targets: [
        { id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt' },
        { id: 'b', name: 'b.txt', path: 'C:\\root\\b.txt' },
      ],
    })
    render(<ActionDialog />)

    expect(screen.getByText('Delete 2 items?')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Delete' }))

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(remove).toHaveBeenCalledWith({ paths: ['C:\\root\\a.txt', 'C:\\root\\b.txt'] })
  })

  it('confirms a transfer and shows what will move from where to where', async () => {
    const user = userEvent.setup()
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)

    useActionDialogStore.getState().open({
      kind: 'transferConfirm',
      paneId: 'left',
      operation: 'move',
      sourceDir: 'C:\\root',
      destinationDir: 'D:\\dest',
      targets: [
        { id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt', sizeBytes: 10 },
        { id: 'b', name: 'b.txt', path: 'C:\\root\\b.txt', sizeBytes: 12 },
      ],
    })
    render(<ActionDialog />)

    expect(screen.getByText('Move 2 items to the other pane?')).toBeInTheDocument()
    expect(screen.getByText('C:\\root')).toBeInTheDocument()
    expect(screen.getByText('D:\\dest')).toBeInTheDocument()
    expect(screen.getByText('a.txt')).toBeInTheDocument()
    expect(screen.getByText('b.txt')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Move' }))

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(startOp).toHaveBeenCalledWith({
      kind: 'move',
      destinationDir: 'D:\\dest',
      items: [
        { sourcePath: 'C:\\root\\a.txt', name: 'a.txt', sizeBytes: 10 },
        { sourcePath: 'C:\\root\\b.txt', name: 'b.txt', sizeBytes: 12 },
      ],
    })
  })

  it('dismisses on cancel without calling the backend', async () => {
    const user = userEvent.setup()
    const create = vi.fn(() => dir('x'))
    ipc.override('create_file', create)

    useActionDialogStore.getState().open({ kind: 'newFile', paneId: 'left' })
    render(<ActionDialog />)

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(useActionDialogStore.getState().dialog).toBeNull()
    expect(create).not.toHaveBeenCalled()
  })
})
