import type { MouseEvent } from 'react'
import { isMiddleButton } from '@/lib/mouse-buttons'

function suppressMiddleButtonAutoscroll(event: MouseEvent) {
  if (isMiddleButton(event)) {
    event.preventDefault()
  }
}

export function middleClickOpenProps(onOpen: () => void) {
  return {
    onMouseDown: suppressMiddleButtonAutoscroll,
    onAuxClick: (event: MouseEvent) => {
      if (!isMiddleButton(event)) {
        return
      }
      event.preventDefault()
      onOpen()
    },
  }
}
