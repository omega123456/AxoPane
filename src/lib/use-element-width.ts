import { useEffect, useState } from 'react'

export function useElementWidth(element: HTMLElement | null) {
  const [width, setWidth] = useState(0)

  useEffect(() => {
    if (!element) {
      return
    }

    const updateWidth = () => {
      const nextWidth = element.getBoundingClientRect().width
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
