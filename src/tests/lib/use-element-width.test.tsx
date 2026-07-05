import { render, screen, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useElementWidth } from '@/lib/use-element-width'

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

  it('reads the mounted element width and disconnects its observer on unmount', async () => {
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
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function mockRect(
      this: HTMLElement,
    ) {
      if (this.dataset.testid === 'target') {
        return DOMRect.fromRect({ width: 123 })
      }

      return DOMRect.fromRect()
    })

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
