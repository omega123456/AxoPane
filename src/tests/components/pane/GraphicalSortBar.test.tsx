import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { GraphicalSortBar } from '@/components/pane/GraphicalSortBar'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneState } from '@/types/pane'

const pane: PaneState = { id: 'left', title: 'Left pane', path: '.', entries: [], focusedEntryId: null, sortKey: 'name', sortDirection: 'asc', filterDraft: '', filterApplied: '', typing: false, loading: false, itemsSortStatus: 'idle', error: null, listRequestId: 0, scrollPositions: {} }

describe('GraphicalSortBar', () => {
  beforeEach(() => usePanesStore.getState().reset())

  it('uses the existing pane sort action for field and direction choices', async () => {
    const user = userEvent.setup()
    const setSort = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ setSort })
    render(<GraphicalSortBar pane={pane} />)

    await user.click(screen.getByRole('button', { name: 'Sort field: Name' }))
    await user.click(screen.getByRole('menuitemradio', { name: 'Size' }))
    expect(setSort).toHaveBeenCalledWith('left', 'size')
    await user.click(screen.getByRole('button', { name: 'Sort direction: Ascending' }))
    await user.click(screen.getByRole('menuitemradio', { name: 'Descending' }))
    expect(setSort).toHaveBeenCalledWith('left', 'name')
  })
})
