import { fireEvent, render, screen } from '@testing-library/react'
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
})
