import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { createRef } from 'react'
import { vi } from 'vitest'
import { GraphicalView, type GraphicalViewHandle } from '@/components/pane/GraphicalView'
import type { FileRowActions } from '@/components/pane/FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneState } from '@/types/pane'

function entries(count: number): DirectoryEntry[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `entry-${index}`,
    name: `Entry ${index}`,
    path: `C:\\root\\Entry ${index}`,
    isDir: false,
    sizeBytes: index,
    itemCount: null,
    typeLabel: 'File',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }))
}

function actions(): FileRowActions {
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
  }
}

function pane(items = entries(10), focusedEntryId: string | null = null): PaneState {
  return {
    id: 'left',
    title: 'Left',
    path: 'C:\\root',
    entries: items,
    focusedEntryId,
    sortKey: 'name',
    sortDirection: 'asc',
    filterDraft: '',
    filterApplied: '',
    typing: false,
    loading: false,
    itemsSortStatus: 'idle',
    error: null,
    listRequestId: 0,
    scrollPositions: {},
  }
}

function renderView(overrides: Partial<React.ComponentProps<typeof GraphicalView>> = {}) {
  const onVisibleRangeChange = vi.fn()
  const onFocusEntry = vi.fn()
  return {
    onVisibleRangeChange,
    onFocusEntry,
    ...render(
      <GraphicalView
        pane={pane()}
        mode="icons"
        isActivePane
        selectedIds={new Set(['entry-1'])}
        cutEntryPaths={new Set()}
        dropTargetEntryId={null}
        rename={null}
        actions={actions()}
        onVisibleRangeChange={onVisibleRangeChange}
        onFocusEntry={onFocusEntry}
        {...overrides}
      />,
    ),
  }
}

describe('GraphicalView', () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      width: 640,
      height: 480,
      top: 0,
      left: 0,
      right: 640,
      bottom: 480,
      toJSON: () => ({}),
    })
  })

  afterEach(() => vi.restoreAllMocks())

  it('uses responsive grid columns and row-major accessibility positions without a Parent item', async () => {
    renderView()
    const grid = screen.getByRole('grid', { name: /Icons for/ })
    await waitFor(() => expect(grid).toHaveAttribute('aria-colcount', '3'))
    expect(screen.getByRole('gridcell', { name: 'Entry 0' })).toHaveAttribute('aria-rowindex', '1')
    expect(screen.getByRole('gridcell', { name: 'Entry 3' })).toHaveAttribute('aria-rowindex', '2')
    expect(screen.queryByText('Go to parent folder')).not.toBeInTheDocument()
  })

  it('falls back to one column when the pane is narrower than one complete cell', async () => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      width: 80,
      height: 480,
      top: 0,
      left: 0,
      right: 80,
      bottom: 480,
      toJSON: () => ({}),
    })
    renderView()
    await waitFor(() => expect(screen.getByRole('grid')).toHaveAttribute('aria-colcount', '1'))
  })

  it('bounds mounted cards to complete visible and overscanned visual rows and reports the entry range', async () => {
    const { onVisibleRangeChange } = renderView({ pane: pane(entries(200)) })
    await waitFor(() => expect(onVisibleRangeChange).toHaveBeenCalled())
    expect(screen.getAllByRole('gridcell').length).toBeLessThan(200)
    expect(onVisibleRangeChange.mock.calls.at(-1)?.[0]).toBe(0)
  })

  it('reports the true thumbnail viewport separately from two-row prefetch', async () => {
    const onThumbnailRangeChange = vi.fn()
    renderView({
      pane: pane(entries(200)),
      mode: 'thumbnails',
      onThumbnailRangeChange,
    })
    await waitFor(() => expect(onThumbnailRangeChange).toHaveBeenCalled())
    expect(onThumbnailRangeChange.mock.calls.at(-1)?.[0]).toEqual({
      visibleStart: 0,
      visibleEnd: 8,
      prefetchStart: 0,
      prefetchEnd: 14,
      direction: 'stationary',
    })
  })

  it('invalidates fixed row measurements when the graphical mode changes', async () => {
    const view = renderView({ pane: pane(entries(20)) })
    await waitFor(() =>
      expect(screen.getByRole('gridcell', { name: 'Entry 3' }).closest('[role="row"]')).toHaveStyle(
        {
          top: '64px',
        },
      ),
    )
    view.rerender(
      <GraphicalView
        pane={pane(entries(20))}
        mode="thumbnails"
        isActivePane
        selectedIds={new Set()}
        cutEntryPaths={new Set()}
        dropTargetEntryId={null}
        rename={null}
        actions={actions()}
        onVisibleRangeChange={view.onVisibleRangeChange}
        onFocusEntry={view.onFocusEntry}
      />,
    )
    await waitFor(() =>
      expect(screen.getByRole('gridcell', { name: 'Entry 3' }).closest('[role="row"]')).toHaveStyle(
        {
          top: '228px',
        },
      ),
    )
  })

  it('moves focus through graphical keyboard geometry and exposes an imperative focused reveal API', async () => {
    const handle = createRef<GraphicalViewHandle>()
    const onKeyboardMove = vi.fn()
    const { onFocusEntry } = renderView({
      ref: handle,
      pane: pane(entries(10), 'entry-4'),
      onKeyboardMove,
    })
    const grid = screen.getByRole('grid')
    await waitFor(() => expect(grid).toHaveAttribute('aria-colcount', '3'))
    fireEvent.keyDown(grid, { key: 'ArrowDown' })
    expect(onFocusEntry).toHaveBeenCalledWith('entry-7')
    expect(onKeyboardMove).toHaveBeenCalledWith('entry-7', 'down')
    expect(handle.current?.getVisibleRowCount()).toBeGreaterThanOrEqual(1)
    expect(() => handle.current?.scrollToEntry(9)).not.toThrow()
  })
})
