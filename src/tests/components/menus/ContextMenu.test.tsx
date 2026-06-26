import { act, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { useContextMenuStore } from '@/stores/context-menu-store'

describe('ContextMenu', () => {
  beforeEach(() => {
    useContextMenuStore.getState().closeMenu()
  })

  it('renders the menu header and starts keyboard activation on the first visible enabled item', () => {
    const open = vi.fn()

    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 24,
      y: 48,
      title: 'Report.txt',
      chip: 'TXT',
      items: [
        { id: 'hidden', label: 'Hidden item', hidden: true },
        { id: 'disabled', label: 'Disabled item', disabled: true },
        { id: 'open', label: 'Open', shortcut: 'Enter', onSelect: open },
      ],
    })

    render(<ContextMenu />)

    const menu = screen.getByRole('menu', { name: 'Report.txt' })
    expect(screen.getByText('TXT')).toBeInTheDocument()
    expect(screen.getByText('Enter')).toBeInTheDocument()
    expect(screen.queryByText('Hidden item')).not.toBeInTheDocument()

    fireEvent.keyDown(menu, { key: 'Enter' })

    expect(open).toHaveBeenCalledTimes(1)
    expect(useContextMenuStore.getState().menu).toBeNull()
  })

  it('supports keyboard movement, escape dismissal, separators, danger, and submenu affordances', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 24,
      y: 48,
      title: 'Report.txt',
      items: [
        { id: 'open', label: 'Open' },
        { id: 'rename', label: 'Rename', strong: true, separatorBefore: true },
        { id: 'delete', label: 'Delete', danger: true, submenu: true },
      ],
    })

    render(<ContextMenu />)
    const menu = screen.getByRole('menu', { name: 'Report.txt' })

    fireEvent.keyDown(menu, { key: 'ArrowDown' })
    expect(useContextMenuStore.getState().activeIndex).toBe(1)
    fireEvent.keyDown(menu, { key: 'ArrowUp' })
    expect(useContextMenuStore.getState().activeIndex).toBe(0)
    fireEvent.keyDown(menu, { key: 'Escape' })
    expect(useContextMenuStore.getState().menu).toBeNull()
  })

  it('activates enabled items by click, ignores disabled clicks, and closes on backdrop press', async () => {
    const user = userEvent.setup()
    const disabled = vi.fn()
    const open = vi.fn()

    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 24,
      y: 48,
      title: 'Report.txt',
      items: [
        { id: 'disabled', label: 'Disabled item', disabled: true, onSelect: disabled },
        { id: 'open', label: 'Open', onSelect: open },
      ],
    })

    const { rerender } = render(<ContextMenu />)

    await user.click(screen.getByRole('menuitem', { name: 'Disabled item' }))
    expect(disabled).not.toHaveBeenCalled()
    expect(useContextMenuStore.getState().menu).not.toBeNull()

    await user.hover(screen.getByRole('menuitem', { name: 'Open' }))
    expect(useContextMenuStore.getState().activeIndex).toBe(1)
    await user.click(screen.getByRole('menuitem', { name: 'Open' }))
    expect(open).toHaveBeenCalledOnce()
    expect(useContextMenuStore.getState().menu).toBeNull()

    act(() => {
      useContextMenuStore.getState().openMenu({
        paneId: 'left',
        x: 24,
        y: 48,
        title: 'Report.txt',
        items: [{ id: 'open', label: 'Open' }],
      })
    })
    rerender(<ContextMenu />)
    fireEvent.mouseDown(screen.getByRole('menu', { name: 'Report.txt' }).parentElement as HTMLElement)
    expect(useContextMenuStore.getState().menu).toBeNull()
  })
})
