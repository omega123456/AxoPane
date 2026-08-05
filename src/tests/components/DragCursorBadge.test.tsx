import { act, render, screen } from '@testing-library/react'
import { beforeEach } from 'vitest'
import { DragCursorBadge } from '@/components/states/DragCursorBadge'
import { useDragCursorStore } from '@/stores/drag-cursor-store'
import { useLayoutStore } from '@/stores/layout-store'

beforeEach(() => {
  act(() => {
    useDragCursorStore.getState().setCursor(null)
    useLayoutStore.setState({ zoom: '100' })
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

  it('cancels the app zoom so it still lands on the pointer', () => {
    act(() => {
      useLayoutStore.setState({ zoom: '150' })
    })
    render(<DragCursorBadge />)

    act(() => {
      useDragCursorStore.getState().setCursor({ x: 300, y: 150, kind: 'copy' })
    })

    // The cursor is in viewport pixels, but every length inside the zoomed root
    // is scaled by 1.5 — so the offsets have to be divided by it to land on the
    // pointer rather than 1.5x further out.
    expect(screen.getByText('Copy').closest('div')).toHaveStyle({
      left: '200px',
      top: '100px',
    })
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
