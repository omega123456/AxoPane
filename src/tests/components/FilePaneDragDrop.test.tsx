import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { endNativeDragBridge, subscribeNativeDragPositions } from '@/lib/native-drag-bridge'
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

function seedPane(
  paneId: PaneId,
  partial: Partial<ReturnType<typeof usePanesStore.getState>['panes']['left']>,
) {
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

/**
 * A controllable `start_native_drag`. The real command resolves only once the OS
 * drag session ends (dropped or cancelled), so it stays pending here until a
 * test explicitly finishes it — which is what keeps the in-app drop tests below
 * exercising a live drag.
 */
function nativeDragSession() {
  const paths: string[][] = []
  let finish = () => {}
  let fail: (error: unknown) => void = () => {}
  const session = new Promise<void>((resolve, reject) => {
    finish = resolve
    fail = reject
  })
  ipc.override('start_native_drag', (payload) => {
    paths.push(payload.paths)
    return session
  })

  return {
    paths,
    async end() {
      finish()
      await act(async () => {
        await session
      })
    },
    async reject(error: unknown) {
      fail(error)
      await act(async () => {
        await session.catch(() => {})
      })
    },
  }
}

let nativeDrag: ReturnType<typeof nativeDragSession>
let unsubscribeDragPositions: Promise<() => void>

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useSelectionStore.getState().reset()
  useDragStore.getState().end()
  nativeDrag = nativeDragSession()
  endNativeDragBridge()
  // `App` owns this subscription in the real app.
  unsubscribeDragPositions = subscribeNativeDragPositions()
})

afterEach(async () => {
  ;(await unsubscribeDragPositions)()
})

/** A position payload that resolves to viewport point (x, y) — no window chrome. */
function dragPosition(x: number, y: number, modifiers: Partial<DragModifierFlags> = {}) {
  const ratio = window.devicePixelRatio || 1
  return {
    cursorX: x * ratio,
    cursorY: y * ratio,
    frameWidth: window.innerWidth * ratio,
    frameHeight: window.innerHeight * ratio,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...modifiers,
  }
}

type DragModifierFlags = {
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
  metaKey: boolean
}

/** Moves the OS drag pointer onto `element`, as Rust's position stream would. */
async function dragPointerOnto(
  element: Element,
  modifiers: Partial<DragModifierFlags> = {},
  x = 10,
  y = 20,
) {
  document.elementFromPoint = vi.fn(() => element)
  await act(async () => {
    ipc.emit('drag://position', dragPosition(x, y, modifiers))
    await Promise.resolve()
  })
}

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

describe('FilePane drag-out to other applications', () => {
  it('hands the grabbed row to the OS as an absolute path', async () => {
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha')] })

    render(<FilePane paneId="left" />)
    const row = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Alpha/ })
    const transfer = dataTransfer()
    fireEvent.dragStart(row, { dataTransfer: transfer })

    await waitFor(() => {
      expect(nativeDrag.paths).toEqual([['C:\\root\\Alpha']])
    })
    // The webview drag is cancelled in favour of the OS one, so nothing is
    // written to `dataTransfer`.
    expect(transfer.setData).not.toHaveBeenCalled()
    // In-app drop targets still resolve their payload from the store.
    expect(useDragStore.getState().drag).toMatchObject({
      kind: 'file-transfer',
      sourceDir: 'C:\\root',
    })
  })

  it('hands the whole selection to the OS when the grabbed row is part of it', async () => {
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha'), entry('Beta'), entry('Gamma')] })
    useSelectionStore.getState().setSelection('left', ['Alpha', 'Gamma'], 'Alpha', 'Gamma')

    render(<FilePane paneId="left" />)
    const pane = within(screen.getByLabelText('Left pane'))
    fireEvent.dragStart(pane.getByRole('row', { name: /Alpha/ }), { dataTransfer: dataTransfer() })

    await waitFor(() => {
      expect(nativeDrag.paths).toEqual([['C:\\root\\Alpha', 'C:\\root\\Gamma']])
    })
  })

  it('clears the drag state when the OS drag ends without an in-app drop', async () => {
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha')] })

    render(<FilePane paneId="left" />)
    const row = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Alpha/ })
    fireEvent.dragStart(row, { dataTransfer: dataTransfer() })
    await waitFor(() => {
      expect(useDragStore.getState().drag).not.toBeNull()
    })

    await nativeDrag.end()
    expect(useDragStore.getState().drag).toBeNull()
  })

  it('clears the drag state when the OS refuses to start the drag', async () => {
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha')] })

    render(<FilePane paneId="left" />)
    const row = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Alpha/ })
    fireEvent.dragStart(row, { dataTransfer: dataTransfer() })
    await waitFor(() => {
      expect(useDragStore.getState().drag).not.toBeNull()
    })

    await nativeDrag.reject(new Error('no drag session'))
    expect(useDragStore.getState().drag).toBeNull()
  })

  it('never starts an OS drag for a trash row, which has no real path', async () => {
    seedPane('left', {
      path: 'C:\\root',
      entries: [{ ...entry('Deleted'), trashId: 'trash-1' }],
    })

    render(<FilePane paneId="left" />)
    const row = within(screen.getByLabelText('Left pane')).getByRole('row', { name: /Deleted/ })
    fireEvent.dragStart(row, { dataTransfer: dataTransfer() })

    await Promise.resolve()
    expect(nativeDrag.paths).toEqual([])
    expect(useDragStore.getState().drag).toBeNull()
  })
})

describe('FilePane in-app drops during an OS-owned drag', () => {
  it('highlights a folder row the OS pointer moves over and drops onto it', async () => {
    const startOp = vi.fn(() => 'op-native-1')
    ipc.override('start_op', startOp)
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha'), entry('Target', true)] })

    render(<FilePane paneId="left" />)
    const pane = within(screen.getByLabelText('Left pane'))
    const source = pane.getByRole('row', { name: /Alpha/ })
    const target = pane.getByRole('row', { name: /Target/ })

    fireEvent.dragStart(source, { dataTransfer: dataTransfer() })
    await waitFor(() => {
      expect(nativeDrag.paths).toEqual([['C:\\root\\Alpha']])
    })

    // The webview sees no DOM drag events for an OS drag; the position stream is
    // what drives the drop target.
    await dragPointerOnto(target)
    expect(target).toHaveClass('ring-accent-blue-border')

    await nativeDrag.end()

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith({
        kind: 'move',
        destinationDir: 'C:\\root\\Target',
        items: [{ sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 }],
      })
    })
    expect(target).not.toHaveClass('ring-accent-blue-border')
    expect(useDragStore.getState().drag).toBeNull()
  })

  it('drops nothing when the drag ends outside the window', async () => {
    const startOp = vi.fn(() => 'op-native-2')
    ipc.override('start_op', startOp)
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha'), entry('Target', true)] })

    render(<FilePane paneId="left" />)
    const pane = within(screen.getByLabelText('Left pane'))
    fireEvent.dragStart(pane.getByRole('row', { name: /Alpha/ }), { dataTransfer: dataTransfer() })
    await waitFor(() => {
      expect(nativeDrag.paths).toHaveLength(1)
    })

    await dragPointerOnto(pane.getByRole('row', { name: /Target/ }))
    // Pointer leaves the webview — the file belongs to whatever app is under it.
    document.elementFromPoint = vi.fn(() => null)
    await act(async () => {
      ipc.emit('drag://position', dragPosition(4000, 20))
      await Promise.resolve()
    })

    await nativeDrag.end()

    expect(startOp).not.toHaveBeenCalled()
    expect(useDragStore.getState().drag).toBeNull()
  })

  it('honours a modifier pressed after the drag has already started', async () => {
    const startOp = vi.fn(() => 'op-native-3')
    ipc.override('start_op', startOp)
    seedPane('left', { path: 'C:\\root', entries: [entry('Alpha'), entry('Target', true)] })

    render(<FilePane paneId="left" />)
    const pane = within(screen.getByLabelText('Left pane'))
    fireEvent.dragStart(pane.getByRole('row', { name: /Alpha/ }), { dataTransfer: dataTransfer() })
    await waitFor(() => {
      expect(nativeDrag.paths).toHaveLength(1)
    })

    // No modifier at drag start; Ctrl goes down mid-gesture, which the OS keyboard
    // grab hides from the webview — it only reaches us through the position stream.
    // Windows convention: Ctrl forces a copy of what would otherwise be a move.
    await dragPointerOnto(pane.getByRole('row', { name: /Target/ }), { ctrlKey: true })
    await nativeDrag.end()

    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith(expect.objectContaining({ kind: 'copy' }))
    })
  })
})
