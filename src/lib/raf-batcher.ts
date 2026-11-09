export type RafBatcher<T> = {
  push: (item: T) => void
  flush: () => void
  cancel: () => void
}

export function createRafBatcher<T>(onFlush: (batch: T[]) => void): RafBatcher<T> {
  let buffer: T[] = []
  let frameHandle: number | null = null

  function flushNow() {
    if (buffer.length === 0) {
      return
    }

    const batch = buffer
    buffer = []
    onFlush(batch)
  }

  function scheduleFlush() {
    if (frameHandle !== null) {
      return
    }

    frameHandle = window.requestAnimationFrame(() => {
      frameHandle = null
      flushNow()
    })
  }

  return {
    push(item: T) {
      buffer.push(item)
      scheduleFlush()
    },
    flush() {
      if (frameHandle !== null) {
        window.cancelAnimationFrame(frameHandle)
        frameHandle = null
      }
      flushNow()
    },
    cancel() {
      if (frameHandle !== null) {
        window.cancelAnimationFrame(frameHandle)
        frameHandle = null
      }
      buffer = []
    },
  }
}
