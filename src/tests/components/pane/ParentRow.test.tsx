import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { vi } from 'vitest'
import { ParentRow } from '@/components/pane/ParentRow'

describe('ParentRow', () => {
  it('focuses on click and activates on double click or Enter', async () => {
    const user = userEvent.setup()
    const onPointerDown = vi.fn()
    const onActivate = vi.fn()
    const onFocus = vi.fn()

    render(
      <ParentRow
        isActivePane
        isFocused
        onPointerDown={onPointerDown}
        onActivate={onActivate}
        onFocus={onFocus}
      />,
    )

    const row = screen.getByRole('row', { name: 'Go to parent folder' })
    await user.click(row)
    expect(onPointerDown).toHaveBeenCalledOnce()
    expect(onFocus).toHaveBeenCalledOnce()

    await user.dblClick(row)
    expect(onActivate).toHaveBeenCalledOnce()
    expect(onActivate).toHaveBeenLastCalledWith(2, expect.any(Number))

    fireEvent.keyDown(row, { key: 'Enter' })
    expect(onActivate).toHaveBeenCalledTimes(2)
    expect(onActivate).toHaveBeenLastCalledWith()
  })

  it('renders without the active focus ring when inactive', () => {
    render(
      <ParentRow
        isActivePane={false}
        isFocused
        onPointerDown={vi.fn()}
        onActivate={vi.fn()}
        onFocus={vi.fn()}
      />,
    )

    expect(screen.getByRole('row', { name: 'Go to parent folder' })).not.toHaveClass('ring-2')
  })
})
