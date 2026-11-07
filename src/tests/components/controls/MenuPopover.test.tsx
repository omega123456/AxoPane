import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { MenuPopover } from '@/components/controls'

describe('MenuPopover', () => {
  it('supports radio state, keyboard navigation, dismissal, and trigger focus restoration', async () => {
    const user = userEvent.setup()
    const selectIcons = vi.fn()
    render(
      <MenuPopover
        ariaLabel="View options"
        radio
        items={[
          { id: 'details', label: 'Details', checked: true, onSelect: vi.fn() },
          { id: 'icons', label: 'Icons', checked: false, onSelect: selectIcons },
        ]}
        trigger={({ ref, expanded, controls, toggle, onTriggerKeyDown }) => (
          <button
            ref={ref}
            type="button"
            aria-label="View"
            aria-expanded={expanded}
            aria-controls={controls}
            onClick={toggle}
            onKeyDown={onTriggerKeyDown}
          >
            View
          </button>
        )}
      />,
    )

    const trigger = screen.getByRole('button', { name: 'View' })
    await user.click(trigger)
    expect(screen.getByRole('menu', { name: 'View options' })).toBeInTheDocument()
    expect(screen.getByRole('menuitemradio', { name: 'Details' })).toHaveAttribute(
      'aria-checked',
      'true',
    )
    await waitFor(() =>
      expect(screen.getByRole('menuitemradio', { name: 'Details' })).toHaveFocus(),
    )
    await user.keyboard('{ArrowDown}{Enter}')
    expect(selectIcons).toHaveBeenCalledOnce()
    await waitFor(() => expect(trigger).toHaveFocus())

    trigger.focus()
    await user.keyboard('{ArrowDown}')
    await waitFor(() =>
      expect(screen.getByRole('menuitemradio', { name: 'Details' })).toHaveFocus(),
    )
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    await waitFor(() => expect(trigger).toHaveFocus())

    await user.click(trigger)
    await user.click(document.body)
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    await waitFor(() => expect(trigger).toHaveFocus())
  })

  it('jumps to the first and last enabled items and closes on Tab', async () => {
    const user = userEvent.setup()
    render(
      <MenuPopover
        ariaLabel="Sort options"
        items={[
          { id: 'name', label: 'Name', onSelect: vi.fn() },
          { id: 'size', label: 'Size', onSelect: vi.fn() },
          { id: 'date', label: 'Date', onSelect: vi.fn() },
        ]}
        trigger={({ ref, toggle }) => (
          <button ref={ref} type="button" aria-label="Sort" onClick={toggle}>
            Sort
          </button>
        )}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'Sort' }))
    await waitFor(() => expect(screen.getByRole('menuitem', { name: 'Name' })).toHaveFocus())
    await user.keyboard('{End}')
    expect(screen.getByRole('menuitem', { name: 'Date' })).toHaveFocus()
    await user.keyboard('{Home}')
    expect(screen.getByRole('menuitem', { name: 'Name' })).toHaveFocus()
    await user.keyboard('{ArrowUp}')
    expect(screen.getByRole('menuitem', { name: 'Date' })).toHaveFocus()
    await user.keyboard('{Tab}')
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })
})
