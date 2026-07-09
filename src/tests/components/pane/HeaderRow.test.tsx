import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { HeaderRow } from '@/components/pane/HeaderRow'
import { ipc } from '@/tests/ipc-mock'
import { usePanesStore } from '@/stores/panes-store'

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
})

describe('HeaderRow items sort pending state', () => {
  it('keeps the Items header interactive and exposes pending sort guidance', async () => {
    const user = userEvent.setup()
    const setSort = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ setSort })
    const pane = {
      ...usePanesStore.getState().panes.left,
      sortKey: 'items' as const,
      sortDirection: 'desc' as const,
      itemsSortStatus: 'counting' as const,
    }

    render(<HeaderRow pane={pane} />)

    const header = screen.getByRole('button', { name: 'Items' })
    const columnHeader = screen.getByRole('columnheader', { name: 'Items' })
    expect(columnHeader).toHaveAttribute('aria-sort', 'descending')
    expect(header).toHaveAccessibleDescription(
      'Counting items. Current row order will update when counting finishes.',
    )

    await user.click(header)
    expect(setSort).toHaveBeenCalledWith('left', 'items')
  })
})
