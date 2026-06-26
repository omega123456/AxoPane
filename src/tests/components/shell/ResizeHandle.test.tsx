import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ResizeHandle } from '@/components/shell/ResizeHandle'

function setup(overrides: Partial<Parameters<typeof ResizeHandle>[0]> = {}) {
  const onDragStart = vi.fn()
  const onResize = vi.fn()
  const onStep = vi.fn()
  const onCommit = vi.fn()
  render(
    <ResizeHandle
      ariaLabel="Resize folder tree"
      value={204}
      min={160}
      max={480}
      onDragStart={onDragStart}
      onResize={onResize}
      onStep={onStep}
      onCommit={onCommit}
      {...overrides}
    />,
  )
  const handle = screen.getByRole('separator', { name: 'Resize folder tree' })
  return { handle, onDragStart, onResize, onStep, onCommit }
}

describe('ResizeHandle', () => {
  it('exposes the current value through ARIA attributes', () => {
    const { handle } = setup()
    expect(handle).toHaveAttribute('aria-orientation', 'vertical')
    expect(handle).toHaveAttribute('aria-valuenow', '204')
    expect(handle).toHaveAttribute('aria-valuemin', '160')
    expect(handle).toHaveAttribute('aria-valuemax', '480')
  })

  it('reports movement relative to the grab point and commits on release', () => {
    const { handle, onDragStart, onResize, onCommit } = setup()
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 200 })
    expect(onDragStart).toHaveBeenCalledTimes(1)
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 320 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 360 })
    // Deltas are measured from the pointerdown X (200), not absolute positions.
    expect(onResize).toHaveBeenNthCalledWith(1, 120)
    expect(onResize).toHaveBeenNthCalledWith(2, 160)
    expect(onCommit).not.toHaveBeenCalled()
    fireEvent.pointerUp(handle, { pointerId: 1 })
    expect(onCommit).toHaveBeenCalledTimes(1)
  })

  it('ignores movement when no drag is active', () => {
    const { handle, onResize, onCommit } = setup()
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 320 })
    expect(onResize).not.toHaveBeenCalled()
    fireEvent.pointerUp(handle, { pointerId: 1 })
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('ignores non-primary pointer buttons', () => {
    const { handle, onResize } = setup()
    fireEvent.pointerDown(handle, { button: 2, pointerId: 1, clientX: 200 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 320 })
    expect(onResize).not.toHaveBeenCalled()
  })

  it('ends the drag on pointer cancel', () => {
    const { handle, onResize, onCommit } = setup()
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 200 })
    fireEvent.pointerCancel(handle, { pointerId: 1 })
    expect(onCommit).toHaveBeenCalledTimes(1)
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 999 })
    expect(onResize).not.toHaveBeenCalled()
  })

  it('nudges with the arrow keys and ignores other keys', () => {
    const { handle, onStep, onCommit } = setup()
    fireEvent.keyDown(handle, { key: 'ArrowLeft' })
    fireEvent.keyDown(handle, { key: 'ArrowRight' })
    expect(onStep).toHaveBeenNthCalledWith(1, -1)
    expect(onStep).toHaveBeenNthCalledWith(2, 1)
    expect(onCommit).toHaveBeenCalledTimes(2)
    fireEvent.keyDown(handle, { key: 'Enter' })
    expect(onStep).toHaveBeenCalledTimes(2)
  })
})
