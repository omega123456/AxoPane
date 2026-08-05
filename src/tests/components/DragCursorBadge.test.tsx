import { act, render, screen } from '@testing-library/react'
import { beforeEach } from 'vitest'
import { DragCursorBadge } from '@/components/states/DragCursorBadge'
import { useDragCursorStore } from '@/stores/drag-cursor-store'

beforeEach(() => {
  act(() => {
    useDragCursorStore.getState().setCursor(null)
  })
})

describe('DragCursorBadge', () => {
  it('renders nothing while no native drag is in flight', () => {
    const { container } = render(<DragCursorBadge />)

    expect(container).toBeEmptyDOMElement()
  })

  it('follows the pointer and names the operation the drop would perform', () => {
    render(<DragCursorBadge />)

    act(() => {
      useDragCursorStore.getState().setCursor({ x: 120, y: 340, kind: 'move' })
    })

    const badge = screen.getByText('Move')
    expect(badge).toBeVisible()
    expect(badge.closest('div')).toHaveStyle({ left: '120px', top: '340px' })
  })

  it('switches label when the modifier flips the drop to a copy', () => {
    render(<DragCursorBadge />)

    act(() => {
      useDragCursorStore.getState().setCursor({ x: 10, y: 10, kind: 'move' })
    })
    expect(screen.getByText('Move')).toBeVisible()

    act(() => {
      useDragCursorStore.getState().setCursor({ x: 10, y: 10, kind: 'copy' })
    })
    expect(screen.getByText('Copy')).toBeVisible()
    expect(screen.queryByText('Move')).not.toBeInTheDocument()
  })

  it('disappears once the drag ends', () => {
    render(<DragCursorBadge />)

    act(() => {
      useDragCursorStore.getState().setCursor({ x: 10, y: 10, kind: 'copy' })
    })
    expect(screen.getByText('Copy')).toBeVisible()

    act(() => {
      useDragCursorStore.getState().setCursor(null)
    })
    expect(screen.queryByText('Copy')).not.toBeInTheDocument()
  })
})
