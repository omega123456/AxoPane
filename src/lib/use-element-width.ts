import { useEffect, useState } from 'react'

export function useElementWidth(element: HTMLElement | null) {
  const [width, setWidth] = useState(0)

  useEffect(() => {
    if (!element) {
      return
    }

    const updateWidth = () => {
      // The computed width is in the element's own CSS px. The bounding rect
      // is scaled by the root `zoom` (Chromium), so it would disagree with
      // CSS px layout maths at any zoom level other than 100%.
      const nextWidth = Number.parseFloat(window.getComputedStyle(element).width) || 0
      setWidth((currentWidth) => (currentWidth === nextWidth ? currentWidth : nextWidth))
    }

    updateWidth()

    const observer = new ResizeObserver(updateWidth)
    observer.observe(element)

    return () => {
      observer.disconnect()
    }
  }, [element])

  return width
}
