import type { MouseEvent } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { middleClickOpenProps } from '@/components/tree/middle-click-open'
import { MIDDLE_MOUSE_BUTTON, PRIMARY_MOUSE_BUTTON } from '@/lib/mouse-buttons'

function mouseEvent(button: number) {
  return { button, preventDefault: vi.fn() } as unknown as MouseEvent & {
    preventDefault: ReturnType<typeof vi.fn>
  }
}

describe('middleClickOpenProps', () => {
  it('suppresses autoscroll only for a middle mouse-down', () => {
    const props = middleClickOpenProps(vi.fn())
    const middleDown = mouseEvent(MIDDLE_MOUSE_BUTTON)
    const primaryDown = mouseEvent(PRIMARY_MOUSE_BUTTON)

    props.onMouseDown(middleDown)
    props.onMouseDown(primaryDown)

    expect(middleDown.preventDefault).toHaveBeenCalledOnce()
    expect(primaryDown.preventDefault).not.toHaveBeenCalled()
  })

  it('opens on a middle click and ignores other buttons', () => {
    const onOpen = vi.fn()
    const props = middleClickOpenProps(onOpen)
    const primaryClick = mouseEvent(PRIMARY_MOUSE_BUTTON)
    const middleClick = mouseEvent(MIDDLE_MOUSE_BUTTON)

    props.onAuxClick(primaryClick)
    expect(onOpen).not.toHaveBeenCalled()
    expect(primaryClick.preventDefault).not.toHaveBeenCalled()

    props.onAuxClick(middleClick)
    expect(onOpen).toHaveBeenCalledOnce()
    expect(middleClick.preventDefault).toHaveBeenCalledOnce()
  })
})
