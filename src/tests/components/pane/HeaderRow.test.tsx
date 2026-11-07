import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { HeaderRow } from '@/components/pane/HeaderRow'
import { ipc } from '@/tests/ipc-mock'
import { usePanesStore } from '@/stores/panes-store'
import { defaultColumnWidths, useLayoutStore } from '@/stores/layout-store'

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

describe('HeaderRow keyboard column resize', () => {
  it('narrows and widens a column with the arrow keys on its separator', async () => {
    const user = userEvent.setup()
    render(<HeaderRow pane={usePanesStore.getState().panes.left} />)

    const separator = screen.getByRole('separator', { name: 'Resize Type column' })
    separator.focus()
    await user.keyboard('{ArrowRight}')
    const widened = useLayoutStore.getState().columnWidths.type
    expect(widened).toBeGreaterThan(defaultColumnWidths.type)
    await user.keyboard('{ArrowLeft}')
    expect(useLayoutStore.getState().columnWidths.type).toBe(defaultColumnWidths.type)
  })
})
