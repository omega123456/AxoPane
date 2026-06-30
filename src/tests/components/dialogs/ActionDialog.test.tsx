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
  ipc.override(
    'list_dir',
    (payload: ListDirRequest): ListDirResponse => ({
      path: payload.path,
      entries: [dir('Reports')],
    }),
  )
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

  it('creates a file on Enter and dismisses prompts on Escape', async () => {
    const user = userEvent.setup()
    const create = vi.fn((payload: { parent: string; name: string }) => dir(payload.name, false))
    ipc.override('create_file', create)

    useActionDialogStore.getState().open({ kind: 'newFile', paneId: 'left' })
    const { unmount } = render(<ActionDialog />)

    await user.type(screen.getByLabelText('File name'), 'notes.txt{Enter}')

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(create).toHaveBeenCalledWith({ parent: 'C:\\root', name: 'notes.txt' })

    unmount()
    useActionDialogStore.getState().open({ kind: 'newFile', paneId: 'left' })
    render(<ActionDialog />)
    await user.keyboard('{Escape}')
    expect(useActionDialogStore.getState().dialog).toBeNull()
  })

  it('confirms a permanent delete and enqueues it through the queue', async () => {
    const user = userEvent.setup()
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)

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
    expect(startOp).toHaveBeenCalledWith({
      kind: 'delete',
      destinationDir: '',
      items: [
        { sourcePath: 'C:\\root\\a.txt', name: 'a.txt', sizeBytes: 0 },
        { sourcePath: 'C:\\root\\b.txt', name: 'b.txt', sizeBytes: 0 },
      ],
    })
  })

  it('renders singular delete copy, handles Escape, and shows delete errors', async () => {
    const user = userEvent.setup()
    ipc.override('start_op', () => {
      throw new Error('delete was denied')
    })

    useActionDialogStore.getState().open({
      kind: 'delete',
      paneId: 'left',
      targets: [{ id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt' }],
    })
    const { unmount } = render(<ActionDialog />)

    expect(screen.getByText('Delete 1 item?')).toBeInTheDocument()
    expect(screen.getByText('a.txt')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Delete' }))
    expect(await screen.findByText('delete was denied')).toBeInTheDocument()
    expect(useActionDialogStore.getState().dialog).not.toBeNull()

    unmount()
    useActionDialogStore.getState().open({
      kind: 'delete',
      paneId: 'left',
      targets: [{ id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt' }],
    })
    render(<ActionDialog />)
    await user.keyboard('{Escape}')
    expect(useActionDialogStore.getState().dialog).toBeNull()
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

  it('renders transfer overflow, submits on Enter, and keeps the dialog open on queue errors', async () => {
    const user = userEvent.setup()
    const startOp = vi.fn(() => {
      throw new Error('queue is unavailable')
    })
    ipc.override('start_op', startOp)

    useActionDialogStore.getState().open({
      kind: 'transferConfirm',
      paneId: 'left',
      operation: 'copy',
      sourceDir: 'C:\\root',
      destinationDir: 'D:\\dest',
      targets: [
        { id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt', sizeBytes: null },
        { id: 'b', name: 'b.txt', path: 'C:\\root\\b.txt', sizeBytes: 12 },
        { id: 'c', name: 'c.txt', path: 'C:\\root\\c.txt', sizeBytes: 14 },
        { id: 'd', name: 'd.txt', path: 'C:\\root\\d.txt', sizeBytes: 16 },
        { id: 'e', name: 'e.txt', path: 'C:\\root\\e.txt', sizeBytes: 18 },
      ],
    })
    const { unmount } = render(<ActionDialog />)

    expect(screen.getByText('Copy 5 items to the other pane?')).toBeInTheDocument()
    expect(screen.getByText('and 1 more')).toBeInTheDocument()

    await user.keyboard('{Enter}')

    expect(await screen.findByText('queue is unavailable')).toBeInTheDocument()
    expect(startOp).toHaveBeenCalledWith({
      kind: 'copy',
      destinationDir: 'D:\\dest',
      items: [
        { sourcePath: 'C:\\root\\a.txt', name: 'a.txt', sizeBytes: 0 },
        { sourcePath: 'C:\\root\\b.txt', name: 'b.txt', sizeBytes: 12 },
        { sourcePath: 'C:\\root\\c.txt', name: 'c.txt', sizeBytes: 14 },
        { sourcePath: 'C:\\root\\d.txt', name: 'd.txt', sizeBytes: 16 },
        { sourcePath: 'C:\\root\\e.txt', name: 'e.txt', sizeBytes: 18 },
      ],
    })
    expect(useActionDialogStore.getState().dialog).not.toBeNull()

    unmount()
    useActionDialogStore.getState().open({
      kind: 'transferConfirm',
      paneId: 'left',
      operation: 'copy',
      sourceDir: 'C:\\root',
      destinationDir: 'D:\\dest',
      targets: [{ id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt' }],
    })
    render(<ActionDialog />)
    await user.keyboard('{Escape}')
    expect(useActionDialogStore.getState().dialog).toBeNull()
  })

  it('confirms archive jobs with an editable archive path or base folder', async () => {
    const user = userEvent.setup()
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)

    useActionDialogStore.getState().open({
      kind: 'archiveConfirm',
      paneId: 'left',
      operation: 'compress',
      destinationDir: 'C:\\root',
      targets: [
        { id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt', sizeBytes: 10 },
        { id: 'b', name: 'b.txt', path: 'C:\\root\\b.txt', sizeBytes: null },
      ],
    })
    const { unmount } = render(<ActionDialog />)

    expect(screen.getByText('Compress 2 items?')).toBeInTheDocument()
    expect(screen.queryByText('Items')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Archive path')).toHaveValue('C:\\root\\Archive.zip')
    await user.clear(screen.getByLabelText('Archive path'))
    await user.type(screen.getByLabelText('Archive path'), 'D:\\archives\\Bundle')
    await user.click(screen.getByRole('button', { name: 'Compress' }))

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(startOp).toHaveBeenCalledWith({
      kind: 'compress',
      destinationDir: 'D:\\archives\\Bundle.zip',
      items: [
        { sourcePath: 'C:\\root\\a.txt', name: 'a.txt', sizeBytes: 10 },
        { sourcePath: 'C:\\root\\b.txt', name: 'b.txt', sizeBytes: 0 },
      ],
    })

    unmount()
    startOp.mockClear()
    useActionDialogStore.getState().open({
      kind: 'archiveConfirm',
      paneId: 'left',
      operation: 'extract',
      destinationDir: 'C:\\root',
      targets: [{ id: 'zip', name: 'Archive.zip', path: 'C:\\root\\Archive.zip', sizeBytes: 900 }],
    })
    render(<ActionDialog />)

    expect(screen.getByText('Extract 1 item?')).toBeInTheDocument()
    expect(screen.queryByText('Items')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Base folder')).toHaveValue('C:\\root')
    await user.keyboard('{Enter}')

    await waitFor(() => expect(useActionDialogStore.getState().dialog).toBeNull())
    expect(startOp).toHaveBeenCalledWith({
      kind: 'extract',
      destinationDir: 'C:\\root',
      items: [{ sourcePath: 'C:\\root\\Archive.zip', name: 'Archive.zip', sizeBytes: 0 }],
    })
  })

  it('keeps archive confirmation open on queue errors and closes on Escape', async () => {
    const user = userEvent.setup()
    ipc.override('start_op', () => {
      throw new Error('archive queue failed')
    })

    useActionDialogStore.getState().open({
      kind: 'archiveConfirm',
      paneId: 'left',
      operation: 'extract',
      destinationDir: 'C:\\root',
      targets: [{ id: 'zip', name: 'Archive.zip', path: 'C:\\root\\Archive.zip' }],
    })
    const { unmount } = render(<ActionDialog />)

    await user.click(screen.getByRole('button', { name: 'Extract' }))
    expect(await screen.findByText('archive queue failed')).toBeInTheDocument()
    expect(useActionDialogStore.getState().dialog).not.toBeNull()

    unmount()
    useActionDialogStore.getState().open({
      kind: 'archiveConfirm',
      paneId: 'left',
      operation: 'compress',
      destinationDir: 'C:\\root',
      targets: [{ id: 'a', name: 'a.txt', path: 'C:\\root\\a.txt' }],
    })
    render(<ActionDialog />)
    await user.keyboard('{Escape}')
    expect(useActionDialogStore.getState().dialog).toBeNull()
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
