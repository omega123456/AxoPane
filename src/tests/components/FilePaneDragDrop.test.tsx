import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { FilePane } from '@/components/pane/FilePane'
import { useDragStore } from '@/stores/drag-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

function entry(name: string, isDir = false, dir = 'C:\\root'): DirectoryEntry {
  return {
    id: name,
    name,
    path: `${dir}\\${name}`,
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

function seedPane(paneId: PaneId, partial: Partial<ReturnType<typeof usePanesStore.getState>['panes']['left']>) {
  usePanesStore.setState((state) => ({
    panes: {
      ...state.panes,
      [paneId]: { ...state.panes[paneId], ...partial },
    },
  }))
}

function dataTransfer() {
  return { setData: vi.fn(), getData: vi.fn(), effectAllowed: '', dropEffect: '' }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useSelectionStore.getState().reset()
  useDragStore.getState().end()
})

describe('FilePane internal drag-and-drop', () => {
  it('moves a dropped item into a folder row on the same volume', async () => {
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha'), entry('Target', true)] })

    render(<FilePane paneId="left" />)
    const pane = within(screen.getByLabelText('Left pane'))
    const source = pane.getByRole('row', { name: /Alpha/ })
    const target = pane.getByRole('row', { name: /Target/ })

    fireEvent.dragStart(source, { dataTransfer: dataTransfer() })
    fireEvent.dragOver(target, { dataTransfer: dataTransfer() })
    // A valid target lights up while hovered.
    expect(target).toHaveClass('ring-accent-blue-border')

    fireEvent.drop(target, { dataTransfer: dataTransfer() })

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith({
        kind: 'move',
        destinationDir: 'C:\\root\\Target',
        items: [{ sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 }],
      })
    })
    // The highlight and active drag are cleared afterwards.
    expect(target).not.toHaveClass('ring-accent-blue-border')
    expect(useDragStore.getState().drag).toBeNull()
  })

  it('copies across volumes when dropped onto the other pane background', async () => {
    const startOp = vi.fn(() => 'op-2')
    ipc.override('start_op', startOp)
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha')] })
    seedPane('right', { path: 'D:\\dest', entries: [entry('Existing', false, 'D:\\dest')] })

    render(
      <>
        <FilePane paneId="left" />
        <FilePane paneId="right" />
      </>,
    )
    const source = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Alpha/ })
    const destScroll = screen.getByTestId('file-pane-scroll-right')

    fireEvent.dragStart(source, { dataTransfer: dataTransfer() })
    fireEvent.dragOver(destScroll, { dataTransfer: dataTransfer() })
    fireEvent.drop(destScroll, { dataTransfer: dataTransfer() })

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith({
        kind: 'copy',
        destinationDir: 'D:\\dest',
        items: [{ sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 }],
      })
    })
  })

  it('drags the whole selection when the grabbed row is part of it', async () => {
    const startOp = vi.fn(() => 'op-3')
    ipc.override('start_op', startOp)
    seedPane('left', {
      path: 'C:\\root',
      entries: [entry('Alpha'), entry('Beta'), entry('Target', true)],
    })
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Beta'], 'Alpha', 'Beta')

    render(<FilePane paneId="left" />)
    const pane = within(screen.getByLabelText('Left pane'))
    fireEvent.dragStart(pane.getByRole('row', { name: /Alpha/ }), { dataTransfer: dataTransfer() })
    fireEvent.drop(pane.getByRole('row', { name: /Target/ }), { dataTransfer: dataTransfer() })

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith(
        expect.objectContaining({
          items: [
            { sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 },
            { sourcePath: 'C:\\root\\Beta', name: 'Beta', sizeBytes: 10 },
          ],
        }),
      )
    })
  })

  it('rejects dropping a folder onto itself', async () => {
    const startOp = vi.fn(() => 'op-4')
    ipc.override('start_op', startOp)
    seedPane('left', { path: 'C:\\root', entries: [entry('Self', true)] })

    render(<FilePane paneId="left" />)
    const self = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Self/ })

    fireEvent.dragStart(self, { dataTransfer: dataTransfer() })
    fireEvent.dragOver(self, { dataTransfer: dataTransfer() })
    // No highlight because the drop is invalid.
    expect(self).not.toHaveClass('ring-accent-blue-border')

    fireEvent.drop(self, { dataTransfer: dataTransfer() })
    await Promise.resolve()
    expect(startOp).not.toHaveBeenCalled()
  })
})
