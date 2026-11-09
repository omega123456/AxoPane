import { useRef } from 'react'
import { persistAppConfig } from '@/lib/app-config'

export type DragResizeSpec = {
  value: number
  stepSize: number
  setValue: (value: number) => void
}

export function commitLayout() {
  void persistAppConfig()
}

export function useDragResize({ value, stepSize, setValue }: DragResizeSpec) {
  const dragStartValue = useRef(value)
  return {
    onDragStart: () => {
      dragStartValue.current = value
    },
    onResize: (delta: number) => setValue(dragStartValue.current + delta),
    onStep: (delta: number) => setValue(value + delta * stepSize),
    onCommit: commitLayout,
  }
}
