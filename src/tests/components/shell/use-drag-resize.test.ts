import { renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { useDragResize } from '@/components/shell/use-drag-resize'
import { ipc } from '@/tests/ipc-mock'

describe('useDragResize', () => {
  it('resizes relative to the value captured when the drag started', () => {
    const setValue = vi.fn()
    const { result, rerender } = renderHook(
      (value: number) => useDragResize({ value, stepSize: 10, setValue }),
      { initialProps: 100 },
    )

    result.current.onDragStart()
    rerender(140)
    result.current.onResize(25)

    expect(setValue).toHaveBeenCalledWith(125)
  })

  it('steps from the current value by the step size', () => {
    const setValue = vi.fn()
    const { result } = renderHook(() => useDragResize({ value: 50, stepSize: 16, setValue }))

    result.current.onStep(-2)

    expect(setValue).toHaveBeenCalledWith(18)
  })

  it('persists the app config on commit', async () => {
    const saved = vi.fn()
    ipc.override('save_config', saved)
    const { result } = renderHook(() => useDragResize({ value: 1, stepSize: 1, setValue: vi.fn() }))

    result.current.onCommit()

    await vi.waitFor(() => expect(saved).toHaveBeenCalled())
  })
})
