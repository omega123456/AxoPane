import { useEffect, useLayoutEffect, useReducer, useState } from 'react'
import {
  Virtualizer,
  elementScroll,
  observeElementOffset,
  observeElementRect,
} from '@tanstack/react-virtual'

type ElementVirtualizerOptions = {
  count: number
  estimateSize: (index: number) => number
  getScrollElement: () => HTMLDivElement | null
  overscan?: number
}

export function useElementVirtualizer(options: ElementVirtualizerOptions) {
  const rerender = useReducer((value) => value + 1, 0)[1]
  const [instance] = useState(
    () =>
      new Virtualizer<HTMLDivElement, HTMLDivElement>({
        count: options.count,
        estimateSize: options.estimateSize,
        getScrollElement: options.getScrollElement,
        observeElementRect,
        observeElementOffset,
        scrollToFn: elementScroll,
        initialRect: {
          width: 0,
          height: 480,
        },
        overscan: options.overscan ?? 10,
        onChange: () => {
          rerender()
        },
      }),
  )

  instance.setOptions({
    count: options.count,
    estimateSize: options.estimateSize,
    getScrollElement: options.getScrollElement,
    observeElementRect,
    observeElementOffset,
    scrollToFn: elementScroll,
    initialRect: {
      width: 0,
      height: 480,
    },
    overscan: options.overscan ?? 10,
    onChange: () => {
      rerender()
    },
  })

  useEffect(() => instance._didMount(), [instance])
  useLayoutEffect(() => instance._willUpdate())
  useLayoutEffect(() => {
    instance.measure()
  }, [instance, options.count])

  return instance
}
