import { act, fireEvent, render, screen } from '@testing-library/react'
import { Profiler } from 'react'
import { afterEach, vi } from 'vitest'
import { EntryCard } from '@/components/pane/EntryCard'
import type { FileRowActions } from '@/components/pane/FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { thumbnailFingerprintKey, useThumbnailStore } from '@/stores/thumbnail-store'

const entry: DirectoryEntry = {
  id: 'photo',
  name: 'A very long photograph filename.png',
  path: 'C:\\photos\\image.png',
  isDir: false,
  sizeBytes: 2,
  itemCount: null,
  typeLabel: 'PNG file',
  modifiedAt: null,
  createdAt: null,
  attributes: ['symlink'],
  isHidden: true,
  isSystem: false,
}
const actions = (): FileRowActions => ({
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
})

describe('EntryCard', () => {
  afterEach(() => {
    act(() => useThumbnailStore.getState().reset())
  })
  it('renders an accessible horizontal Icons tile and forwards entry actions', () => {
    const cardActions = actions()
    render(
      <EntryCard
        entry={entry}
        mode="icons"
        isActivePane
        isFocused
        isSelected
        isCut
        isDropTarget
        rowIndex={2}
        columnIndex={3}
        actions={cardActions}
      />,
    )
    const card = screen.getByRole('gridcell', { name: entry.name })
    expect(card).toHaveAttribute('title', entry.name)
    expect(card).toHaveAttribute('aria-rowindex', '2')
    expect(card).toHaveAttribute('aria-colindex', '3')
    expect(card).toHaveAttribute('aria-selected', 'true')
    expect(card).toHaveClass('opacity-50')
    expect(card.querySelector('[data-symlink="true"]')).toBeTruthy()
    fireEvent.click(card)
    fireEvent.doubleClick(card, { detail: 2 })
    fireEvent.contextMenu(card)
    expect(cardActions.onClick).toHaveBeenCalledWith('photo', expect.anything())
    expect(cardActions.onActivate).toHaveBeenCalledWith('photo', 2, expect.any(Number))
    expect(cardActions.onContextMenu).toHaveBeenCalledWith('photo', expect.anything())
  })

  it('renders ready, loading, and fallback thumbnail states', () => {
    const { rerender } = render(
      <EntryCard
        entry={entry}
        mode="thumbnails"
        isActivePane={false}
        isFocused={false}
        isSelected={false}
        rowIndex={1}
        columnIndex={1}
        actions={actions()}
        thumbnail={{ state: 'ready', dataUrl: 'data:image/png;base64,AA==' }}
      />,
    )
    expect(screen.getByRole('img', { hidden: true })).toHaveAttribute(
      'src',
      'data:image/png;base64,AA==',
    )
    rerender(
      <EntryCard
        entry={entry}
        mode="thumbnails"
        isActivePane={false}
        isFocused={false}
        isSelected={false}
        rowIndex={1}
        columnIndex={1}
        actions={actions()}
        thumbnail={{ state: 'loading' }}
      />,
    )
    expect(screen.getByLabelText('Thumbnail loading')).toBeInTheDocument()
    rerender(
      <EntryCard
        entry={entry}
        mode="thumbnails"
        isActivePane={false}
        isFocused={false}
        isSelected={false}
        rowIndex={1}
        columnIndex={1}
        actions={actions()}
        thumbnail={{ state: 'failed' }}
      />,
    )
    expect(screen.queryByLabelText('Thumbnail loading')).not.toBeInTheDocument()
  })

  it('rerenders only the preview whose fingerprint changed', async () => {
    const first = { ...entry, modifiedAt: '1970-01-01T00:00:20Z' }
    const second = {
      ...first,
      id: 'other',
      name: 'Other.png',
      path: 'C:\\photos\\other.png',
    }
    const firstRender = vi.fn()
    const secondRender = vi.fn()
    render(
      <>
        <Profiler id="first" onRender={firstRender}>
          <EntryCard
            entry={first}
            mode="thumbnails"
            isActivePane
            isFocused={false}
            isSelected={false}
            rowIndex={1}
            columnIndex={1}
            actions={actions()}
          />
        </Profiler>
        <Profiler id="second" onRender={secondRender}>
          <EntryCard
            entry={second}
            mode="thumbnails"
            isActivePane
            isFocused={false}
            isSelected={false}
            rowIndex={1}
            columnIndex={2}
            actions={actions()}
          />
        </Profiler>
      </>,
    )
    firstRender.mockClear()
    secondRender.mockClear()
    const key = thumbnailFingerprintKey({
      path: first.path,
      modifiedUnixSeconds: 20,
      sizeBytes: first.sizeBytes ?? 0,
    })
    await act(async () => {
      useThumbnailStore.setState((state) => ({
        cache: {
          ...state.cache,
          [key]: {
            state: 'ready',
            quality: 'low',
            dataUrl: 'data:image/png;base64,AA==',
            touched: 1,
            weight: 2,
          },
        },
      }))
    })
    expect(firstRender).toHaveBeenCalledOnce()
    expect(secondRender).not.toHaveBeenCalled()
  })

  it('centers a larger thumbnail label beneath its larger preview stage', () => {
    render(
      <EntryCard
        entry={entry}
        mode="thumbnails"
        isActivePane={false}
        isFocused={false}
        isSelected={false}
        rowIndex={1}
        columnIndex={1}
        actions={actions()}
      />,
    )
    const card = screen.getByRole('gridcell', { name: entry.name })
    expect(card).toHaveClass('h-thumbnail-card', 'min-w-thumbnail-cell')
    expect(card.querySelector('.size-thumbnail-preview')).toBeTruthy()
    expect(card.querySelector('.text-center')).toHaveTextContent(entry.name)
  })

  it('presents inline rename with submit, cancel, and error feedback', () => {
    const cardActions = actions()
    render(
      <EntryCard
        entry={entry}
        mode="thumbnails"
        isActivePane
        isFocused={false}
        isSelected={false}
        rowIndex={1}
        columnIndex={1}
        actions={cardActions}
        isRenaming
        renameValue={entry.name}
        renameError="Name exists"
      />,
    )
    const input = screen.getByRole('textbox', { name: `Rename ${entry.name}` })
    expect(input).toHaveFocus()
    expect(input).toHaveProperty('selectionStart', 0)
    expect(input).toHaveProperty('selectionEnd', entry.name.lastIndexOf('.'))
    expect(screen.getByText('Name exists')).toBeInTheDocument()
    fireEvent.change(input, { target: { value: 'Changed' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    fireEvent.keyDown(input, { key: 'Escape' })
    expect(cardActions.onRenameChange).toHaveBeenCalledWith('Changed')
    expect(cardActions.onRenameSubmit).toHaveBeenCalledOnce()
    expect(cardActions.onRenameCancel).toHaveBeenCalledOnce()
  })
})
