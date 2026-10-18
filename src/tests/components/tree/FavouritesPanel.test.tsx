import { act, createEvent, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { FavouritesPanel } from '@/components/tree/FavouritesPanel'
import { useConfigStore } from '@/stores/config-store'
import { useDragStore } from '@/stores/drag-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'

const FAVOURITES = [
  'C:\\Users\\Omega\\Documents',
  'C:\\Users\\Omega\\Downloads',
  'D:\\projects',
  'E:\\',
  'C:\\Users\\Omega\\Music',
]

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useDragStore.getState().end()
  useConfigStore.getState().reset()
  useLayoutStore.getState().reset()
})

describe('FavouritesPanel', () => {
  it('collapses to its heading with a count and saves the collapsed state', async () => {
    const user = userEvent.setup()
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)
    useConfigStore.setState({ favourites: FAVOURITES })
    render(<FavouritesPanel />)

    const heading = screen.getByRole('button', { name: 'Favourites' })
    expect(heading).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByRole('button', { name: 'Documents' })).toBeInTheDocument()

    await user.click(heading)

    expect(heading).toHaveAttribute('aria-expanded', 'false')
    expect(heading).toHaveTextContent('Favourites5')
    expect(screen.queryByRole('button', { name: 'Documents' })).not.toBeInTheDocument()
    expect(saveConfig).toHaveBeenCalledWith(
      expect.objectContaining({ config: expect.objectContaining({ favouritesCollapsed: true }) }),
    )

    await user.click(heading)
    expect(screen.getByRole('button', { name: 'Documents' })).toBeInTheDocument()
  })

  it('scrolls favourites inside a capped list of their own', () => {
    useConfigStore.setState({ favourites: FAVOURITES })
    render(<FavouritesPanel />)

    const list = screen
      .getByRole('button', { name: 'Documents' })
      .closest('[data-favourite-row]')?.parentElement
    expect(list).toHaveStyle({ maxHeight: '112px' })
    expect(list?.className).toContain('overflow-y-auto')
  })

  it('resizes the list from its shown height and persists the result', async () => {
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)
    useConfigStore.setState({ favourites: FAVOURITES.slice(0, 2) })
    act(() => useLayoutStore.getState().setFavouritesHeightPx(300))
    render(<FavouritesPanel />)

    // Two 28px rows show 56px of a 300px cap; the drag starts from what is shown.
    const handle = screen.getByRole('separator', { name: 'Resize favourites' })
    expect(handle).toHaveAttribute('aria-valuenow', '56')
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientY: 100 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 80 })
    expect(useLayoutStore.getState().favouritesHeightPx).toBe(36)

    fireEvent.pointerUp(handle, { pointerId: 1 })
    await waitFor(() =>
      expect(saveConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          config: expect.objectContaining({
            layout: expect.objectContaining({ favouritesHeightPx: 36 }),
          }),
        }),
      ),
    )

    fireEvent.keyDown(handle, { key: 'ArrowDown' })
    expect(useLayoutStore.getState().favouritesHeightPx).toBe(64)
  })

  it('hides the resize handle while collapsed or empty', () => {
    useConfigStore.setState({ favourites: FAVOURITES, favouritesCollapsed: true })
    const { unmount } = render(<FavouritesPanel />)
    expect(screen.queryByRole('separator')).not.toBeInTheDocument()
    unmount()

    act(() => useConfigStore.setState({ favourites: [], favouritesCollapsed: false }))
    render(<FavouritesPanel />)
    expect(screen.queryByRole('separator')).not.toBeInTheDocument()
  })

  it('marks the favourite for the active pane folder', () => {
    useConfigStore.setState({ favourites: FAVOURITES })
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, left: { ...state.panes.left, path: 'D:\\projects' } },
    }))
    render(<FavouritesPanel />)

    const row = screen.getByRole('button', { name: 'projects' }).closest('[data-favourite-row]')
    expect(row?.className).toContain('bg-light-hover')
  })

  it('accepts folder drops on the heading while collapsed and ignores other drags', async () => {
    useConfigStore.setState({ favourites: ['C:\\A'], favouritesCollapsed: true })
    render(<FavouritesPanel />)
    const heading = screen.getByRole('button', { name: /Favourites/ })

    const ignored = createEvent.dragOver(heading)
    fireEvent(heading, ignored)
    expect(ignored.defaultPrevented).toBe(false)
    fireEvent.drop(heading)
    expect(useConfigStore.getState().favourites).toEqual(['C:\\A'])

    useDragStore.getState().begin({
      kind: 'file-transfer',
      sourcePaneId: 'left',
      sourceDir: 'C:\\root',
      items: [{ id: 'dir', name: 'B', path: 'C:\\B', isDir: true, sizeBytes: null }],
    })
    const accepted = createEvent.dragOver(heading, { dataTransfer: { dropEffect: '' } })
    fireEvent(heading, accepted)
    expect(accepted.defaultPrevented).toBe(true)
    fireEvent.drop(heading, { dataTransfer: { dropEffect: '' } })

    await waitFor(() => expect(useConfigStore.getState().favourites).toEqual(['C:\\A', 'C:\\B']))
  })

  it('moves a favourite dropped on the heading to the end of the list', async () => {
    useConfigStore.setState({ favourites: ['C:\\A', 'C:\\B', 'C:\\C'] })
    render(<FavouritesPanel />)

    act(() => useDragStore.getState().begin({ kind: 'favourite', path: 'C:\\A' }))
    fireEvent.drop(screen.getByRole('button', { name: 'Favourites' }), {
      dataTransfer: { dropEffect: '' },
    })

    await waitFor(() =>
      expect(useConfigStore.getState().favourites).toEqual(['C:\\B', 'C:\\C', 'C:\\A']),
    )
  })
})
