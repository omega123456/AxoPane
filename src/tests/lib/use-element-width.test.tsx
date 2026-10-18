import { render, screen, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useElementWidth } from '@/lib/use-element-width'
import { mockComputedWidth } from '@/tests/element-width'

function WidthProbe() {
  const [element, setElement] = useState<HTMLDivElement | null>(null)
  const width = useElementWidth(element)

  return (
    <div>
      <div ref={setElement} data-testid="target" />
      <output aria-label="Width" role="status">
        {width}
      </output>
    </div>
  )
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe('useElementWidth', () => {
  it('defaults to zero when the element has no measurable width', () => {
    render(<WidthProbe />)

    expect(screen.getByRole('status', { name: 'Width' })).toHaveTextContent('0')
  })

  it('reads the unzoomed CSS width and disconnects its observer on unmount', async () => {
    const disconnect = vi.fn()
    const observe = vi.fn()
    const originalResizeObserver = window.ResizeObserver

    Object.defineProperty(window, 'ResizeObserver', {
      configurable: true,
      writable: true,
      value: class ResizeObserverStub {
        observe = observe

        disconnect = disconnect

        constructor(callback: ResizeObserverCallback) {
          void callback
        }
      },
    })
    mockComputedWidth(123, (element) => element.getAttribute('data-testid') === 'target')
    // A 125% root `zoom` scales the bounding rect but not the CSS layout width
    // the breadcrumb and grid maths run in.
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(
      DOMRect.fromRect({ width: 153.75 }),
    )

    const view = render(<WidthProbe />)

    await waitFor(() => {
      expect(screen.getByRole('status', { name: 'Width' })).toHaveTextContent('123')
    })
    view.unmount()

    expect(observe).toHaveBeenCalled()
    expect(disconnect).toHaveBeenCalled()

    Object.defineProperty(window, 'ResizeObserver', {
      configurable: true,
      writable: true,
      value: originalResizeObserver,
    })
  })
})
