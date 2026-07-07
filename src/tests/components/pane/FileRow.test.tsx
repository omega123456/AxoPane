import { fireEvent, render, screen } from '@testing-library/react'
import { parseISO } from 'date-fns'
import { useState } from 'react'
import { afterEach, beforeEach, vi } from 'vitest'
import { FileRow, type FileRowActions } from '@/components/pane/FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import * as configStoreModule from '@/stores/config-store'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'

function makeEntry(overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    id: 'doc',
    name: 'Doc.txt',
    path: 'C:\\root\\Doc.txt',
    isDir: false,
    sizeBytes: 2048,
    itemCount: null,
    typeLabel: 'TXT file',
    modifiedAt: '2026-06-20T14:05:09Z',
    createdAt: '2026-06-01T09:00:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
    ...overrides,
  }
}

function makeActions(overrides: Partial<FileRowActions> = {}): FileRowActions {
  return {
    onPointerDown: vi.fn(),
    onActivate: vi.fn(),
    onClick: vi.fn(),
    onContextMenu: vi.fn(),
    onMiddleClick: vi.fn(),
    onDragStart: vi.fn(),
    onDragEnd: vi.fn(),
    onDragEnter: vi.fn(),
    onDragOver: vi.fn(),
    onDragLeave: vi.fn(),
    onDrop: vi.fn(),
    onRenameChange: vi.fn(),
    onRenameSubmit: vi.fn(),
    onRenameCancel: vi.fn(),
    onRenameBlur: vi.fn(),
    ...overrides,
  }
}

beforeEach(() => {
  useLayoutStore.getState().reset()
  useConfigStore.getState().reset()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('FileRow date column', () => {
  it('renders the configured absolute format', () => {
    useConfigStore.setState({ dateFormat: 'dmy', relativeDates: false })

    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions()}
      />,
    )

    expect(screen.getByText('20/06/2026')).toBeInTheDocument()
  })

  it('colour-codes a recent modified date when relative dates are enabled', () => {
    vi.useFakeTimers()
    vi.setSystemTime(parseISO('2026-06-30T12:00:00Z'))
    useConfigStore.setState({ dateFormat: 'ymd', relativeDates: true })

    render(
      <FileRow
        entry={makeEntry({ modifiedAt: '2026-06-30T10:00:00Z' })}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions()}
      />,
    )

    const cell = screen.getByText('2 hours ago')
    expect(cell).toHaveClass('text-accent-blue-light')
  })

  it('shows a weekday label for modified dates from earlier in the past week', () => {
    vi.useFakeTimers()
    vi.setSystemTime(parseISO('2026-06-30T12:00:00Z'))
    useConfigStore.setState({
      dateFormat: 'ymd',
      relativeDates: true,
      showTime: true,
      showSeconds: false,
    })

    render(
      <FileRow
        entry={makeEntry({ modifiedAt: '2026-06-26T10:00:00Z' })}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions()}
      />,
    )

    expect(screen.getByText('on Friday 10:00')).toBeInTheDocument()
  })
})

describe('FileRow drag-and-drop', () => {
  it('marks the row draggable and forwards the drag-start event bound to the entry id', () => {
    const onDragStart = vi.fn()
    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        draggable
        actions={makeActions({ onDragStart })}
      />,
    )

    const row = screen.getByRole('row')
    expect(row).toHaveAttribute('draggable', 'true')

    fireEvent.dragStart(row, { dataTransfer: { setData: vi.fn(), effectAllowed: '' } })
    expect(onDragStart).toHaveBeenCalledOnce()
    expect(onDragStart.mock.calls[0][0]).toBe('doc')
  })

  it('highlights the row while it is an active drop target', () => {
    const { rerender } = render(
      <FileRow
        entry={makeEntry({ isDir: true })}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions()}
      />,
    )
    expect(screen.getByRole('row')).not.toHaveClass('ring-accent-blue-border')

    rerender(
      <FileRow
        entry={makeEntry({ isDir: true })}
        isActivePane
        isFocused={false}
        isSelected={false}
        isDropTarget
        actions={makeActions()}
      />,
    )
    expect(screen.getByRole('row')).toHaveClass('ring-accent-blue-border')
  })
})

describe('FileRow action dispatcher', () => {
  it('binds the entry id when forwarding click, context-menu, and activation events', () => {
    const onClick = vi.fn()
    const onContextMenu = vi.fn()
    const onActivate = vi.fn()
    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions({ onClick, onContextMenu, onActivate })}
      />,
    )

    const row = screen.getByRole('row')
    fireEvent.click(row)
    fireEvent.contextMenu(row)
    fireEvent.doubleClick(row)

    expect(onClick.mock.calls[0][0]).toBe('doc')
    expect(onContextMenu.mock.calls[0][0]).toBe('doc')
    expect(onActivate.mock.calls[0][0]).toBe('doc')
  })

  it('forwards pointer-down and drag-end without an entry id (pane-wide handlers)', () => {
    const onPointerDown = vi.fn()
    const onDragEnd = vi.fn()
    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions({ onPointerDown, onDragEnd })}
      />,
    )

    const row = screen.getByRole('row')
    fireEvent.mouseDown(row)
    fireEvent.dragEnd(row, { dataTransfer: { setData: vi.fn(), effectAllowed: '' } })

    expect(onPointerDown).toHaveBeenCalledOnce()
    expect(onDragEnd).toHaveBeenCalledOnce()
  })

  it('invokes middle-click only for directory rows without a trashId', () => {
    const onMiddleClick = vi.fn()
    render(
      <FileRow
        entry={makeEntry({ isDir: true })}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions({ onMiddleClick })}
      />,
    )

    fireEvent(
      screen.getByRole('row'),
      new MouseEvent('auxclick', { button: 1, bubbles: true, cancelable: true }),
    )

    expect(onMiddleClick).toHaveBeenCalledWith('doc')
  })

  it('does not invoke middle-click for a non-directory row', () => {
    const onMiddleClick = vi.fn()
    render(
      <FileRow
        entry={makeEntry({ isDir: false })}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions({ onMiddleClick })}
      />,
    )

    fireEvent(
      screen.getByRole('row'),
      new MouseEvent('auxclick', { button: 1, bubbles: true, cancelable: true }),
    )

    expect(onMiddleClick).not.toHaveBeenCalled()
  })

  it('routes drag-enter/over/leave/drop through the dispatcher bound to the entry id', () => {
    const onDragEnter = vi.fn()
    const onDragOver = vi.fn()
    const onDragLeave = vi.fn()
    const onDrop = vi.fn()
    render(
      <FileRow
        entry={makeEntry({ isDir: true })}
        isActivePane
        isFocused={false}
        isSelected={false}
        actions={makeActions({ onDragEnter, onDragOver, onDragLeave, onDrop })}
      />,
    )

    const row = screen.getByRole('row')
    const dataTransfer = { setData: vi.fn(), getData: vi.fn(), effectAllowed: '', dropEffect: '' }
    fireEvent.dragEnter(row, { dataTransfer })
    fireEvent.dragOver(row, { dataTransfer })
    fireEvent.dragLeave(row, { dataTransfer })
    fireEvent.drop(row, { dataTransfer })

    expect(onDragEnter.mock.calls[0][0]).toBe('doc')
    expect(onDragOver.mock.calls[0][0]).toBe('doc')
    expect(onDragLeave.mock.calls[0][0]).toBe('doc')
    expect(onDrop.mock.calls[0][0]).toBe('doc')
  })
})

describe('FileRow rename mode', () => {
  it('routes rename input change/submit/cancel/blur through the dispatcher', () => {
    const onRenameChange = vi.fn()
    const onRenameSubmit = vi.fn()
    const onRenameCancel = vi.fn()
    const onRenameBlur = vi.fn()
    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        isRenaming
        renameValue="Doc"
        actions={makeActions({
          onRenameChange,
          onRenameSubmit,
          onRenameCancel,
          onRenameBlur,
        })}
      />,
    )

    const input = screen.getByLabelText('Rename Doc.txt')
    fireEvent.change(input, { target: { value: 'Doc2' } })
    expect(onRenameChange).toHaveBeenCalledWith('Doc2')

    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onRenameSubmit).toHaveBeenCalledOnce()

    fireEvent.keyDown(input, { key: 'Escape' })
    expect(onRenameCancel).toHaveBeenCalledOnce()

    fireEvent.blur(input)
    expect(onRenameBlur).toHaveBeenCalledOnce()
  })

  it('shows a renaming-error state without invoking any dispatcher handler', () => {
    render(
      <FileRow
        entry={makeEntry()}
        isActivePane
        isFocused={false}
        isSelected={false}
        isRenaming
        renameValue="Doc"
        renameError="Name already exists"
        actions={makeActions()}
      />,
    )

    expect(screen.getByText('Name already exists')).toBeInTheDocument()
  })
})

describe('FileRow memoization', () => {
  it('is wrapped in React.memo', () => {
    // `FileRow` is exported as the memoized component itself. React.memo
    // components carry a `$$typeof` of Symbol.for('react.memo').
    expect(String((FileRow as unknown as { $$typeof: symbol }).$$typeof)).toBe(
      'Symbol(react.memo)',
    )
  })

  it('skips re-rendering when a parent re-renders but every FileRow prop stays referentially equal, and re-renders once a prop genuinely changes', () => {
    // Regression test for the row-memoization goal: a `FilePane` re-render
    // triggered by unrelated state (e.g. a sibling row's focus change) must
    // not re-render every visible row. `React.memo`'s bail-out means the
    // wrapped `FileRowImpl` function body never runs at all when props are
    // unchanged, so every hook call inside it — including
    // `useConfigStore(...)`, called unconditionally on every real render —
    // is skipped too. Spying on that store hook therefore counts *actual*
    // FileRow render invocations, independent of any internal implementation
    // detail beyond "FileRow calls useConfigStore every time it renders".
    const configStoreSpy = vi.spyOn(configStoreModule, 'useConfigStore')
    const renderCountAtStart = configStoreSpy.mock.calls.length
    const actions = makeActions()
    const entry = makeEntry()

    function Harness() {
      const [tick, setTick] = useState(0)
      const [isFocused, setIsFocused] = useState(false)
      return (
        <div data-tick={tick}>
          <button onClick={() => setTick((value) => value + 1)}>bump unrelated state</button>
          <button onClick={() => setIsFocused(true)}>focus this row</button>
          <FileRow
            entry={entry}
            isActivePane
            isFocused={isFocused}
            isSelected={false}
            actions={actions}
          />
        </div>
      )
    }

    render(<Harness />)
    // FileRowImpl reads 4 fields off useConfigStore per real render (dateFormat,
    // showTime, showSeconds, relativeDates).
    const callsPerRender = 4
    const rendersSoFar = () =>
      (configStoreSpy.mock.calls.length - renderCountAtStart) / callsPerRender

    expect(rendersSoFar()).toBe(1)

    // Unrelated parent state changes (tick) — every FileRow prop is still the
    // exact same reference/value, so the memoized row must not re-render.
    fireEvent.click(screen.getByText('bump unrelated state'))
    fireEvent.click(screen.getByText('bump unrelated state'))
    expect(rendersSoFar()).toBe(1)

    // A genuine prop change (this row becoming focused) must still re-render it.
    fireEvent.click(screen.getByText('focus this row'))
    expect(rendersSoFar()).toBe(2)
  })
})
