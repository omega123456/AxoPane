import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { vi } from 'vitest'
import { DetailsView } from '@/components/pane/DetailsView'
import type { FileRowActions } from '@/components/pane/FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneState } from '@/types/pane'

function entry(index: number): DirectoryEntry {
  return {
    id: `entry-${index}`,
    name: `Entry ${index}.txt`,
    path: `C:\\root\\Entry ${index}.txt`,
    isDir: index === 0,
    sizeBytes: index === 0 ? null : index,
    itemCount: index === 0 ? 3 : null,
    typeLabel: index === 0 ? 'Folder' : 'TXT file',
    modifiedAt: '2026-06-20T14:05:09Z',
    createdAt: '2026-06-01T09:00:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

function pane(entries: DirectoryEntry[] = [entry(0), entry(1)]): PaneState {
  return {
    ...usePanesStore.getState().panes.left,
    path: 'C:\\root',
    entries,
    focusedEntryId: null,
  }
}

function actions(overrides: Partial<FileRowActions> = {}): FileRowActions {
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

function renderDetails(overrides: Partial<ComponentProps<typeof DetailsView>> = {}) {
  const onVisibleRangeChange = vi.fn()
  return {
    onVisibleRangeChange,
    ...render(
      <DetailsView
        pane={pane()}
        isActivePane
        hasParent
        selectedIds={new Set()}
        cutEntryPaths={new Set()}
        dropTargetEntryId={null}
        isPaneDropTarget={false}
        rename={null}
        actions={actions()}
        usesDetachedMacScrollbars={false}
        marqueeRect={null}
        onVisibleRangeChange={onVisibleRangeChange}
        onContainerMouseDown={vi.fn()}
        onScroll={vi.fn()}
        onContainerContextMenu={vi.fn()}
        onPaneDragOver={vi.fn()}
        onPaneDragLeave={vi.fn()}
        onPaneDrop={vi.fn()}
        onParentFocus={vi.fn()}
        onParentActivate={vi.fn()}
        {...overrides}
      />,
    ),
  }
}

beforeEach(() => {
  usePanesStore.getState().reset()
  useLayoutStore.getState().reset()
  useConfigStore.getState().reset()
})

describe('DetailsView', () => {
  it('renders the existing header, parent row, and directory rows', () => {
    renderDetails()

    expect(screen.getByRole('columnheader', { name: 'Name' })).toBeInTheDocument()
    expect(screen.getByRole('row', { name: 'Go to parent folder' })).toBeInTheDocument()
    expect(screen.getByRole('row', { name: /Entry 0.txt/ })).toBeInTheDocument()
    expect(screen.getByRole('row', { name: /Entry 1.txt/ })).toBeInTheDocument()
  })

  it('keeps the unmeasured virtualized fallback bounded and reports entry-only visible ranges', async () => {
    const entries = Array.from({ length: 200 }, (_, index) => entry(index))
    const { onVisibleRangeChange } = renderDetails({ pane: pane(entries) })

    expect(screen.getAllByRole('row')).toHaveLength(30)
    await waitFor(() => expect(onVisibleRangeChange).toHaveBeenCalled())
    expect(onVisibleRangeChange.mock.calls.at(-1)).toEqual([0, 28, 29])
  })

  it('synchronizes detached horizontal columns and exposes the horizontal scrollbar', () => {
    renderDetails({ usesDetachedMacScrollbars: true })

    const header = screen.getByTestId('file-pane-header-scroll-left')
    const content = screen.getByTestId('file-pane-scroll-left').firstElementChild as HTMLDivElement
    const horizontalScroll = screen.getByTestId('file-pane-horizontal-scroll-left')
    fireEvent.scroll(horizontalScroll, { target: { scrollLeft: 48 } })

    expect(header.scrollLeft).toBe(48)
    expect(content.style.transform).toBe('translateX(-48px)')
  })

  it('forwards row actions through the supplied stable dispatcher', () => {
    const onClick = vi.fn()
    const onContextMenu = vi.fn()
    const onActivate = vi.fn()
    const stableActions = actions({ onClick, onContextMenu, onActivate })
    const { rerender } = renderDetails({ actions: stableActions })
    const row = screen.getByRole('row', { name: /Entry 1.txt/ })

    fireEvent.click(row)
    fireEvent.contextMenu(row)
    fireEvent.doubleClick(row)
    expect(onClick.mock.calls[0][0]).toBe('entry-1')
    expect(onContextMenu.mock.calls[0][0]).toBe('entry-1')
    expect(onActivate.mock.calls[0][0]).toBe('entry-1')

    rerender(
      <DetailsView
        pane={pane([entry(0), entry(1)])}
        isActivePane
        hasParent
        selectedIds={new Set()}
        cutEntryPaths={new Set()}
        dropTargetEntryId={null}
        isPaneDropTarget={false}
        rename={null}
        actions={stableActions}
        usesDetachedMacScrollbars={false}
        marqueeRect={null}
        onVisibleRangeChange={vi.fn()}
        onContainerMouseDown={vi.fn()}
        onScroll={vi.fn()}
        onContainerContextMenu={vi.fn()}
        onPaneDragOver={vi.fn()}
        onPaneDragLeave={vi.fn()}
        onPaneDrop={vi.fn()}
        onParentFocus={vi.fn()}
        onParentActivate={vi.fn()}
      />,
    )
    fireEvent.click(screen.getByRole('row', { name: /Entry 1.txt/ }))
    expect(onClick).toHaveBeenCalledTimes(2)
  })
})
