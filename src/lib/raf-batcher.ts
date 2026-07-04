/**
 * A small, reusable requestAnimationFrame-coalescing helper.
 *
 * Consumers `push` items onto the batcher as they arrive (e.g. from a burst
 * of IPC events); the batcher accumulates them and invokes a single `onFlush`
 * callback with the whole accumulated batch once per animation frame, so a
 * burst of N events during one frame produces exactly one downstream update
 * instead of N.
 *
 * Pure utility: no store or IPC knowledge. Exposes a synchronous `flush()` so
 * tests (including those under fake timers, where a real animation frame
 * never fires) can deterministically drain the buffer without waiting on
 * `requestAnimationFrame`.
 */
export type RafBatcher<T> = {
  /** Accumulate an item; schedules a flush for the next animation frame if one isn't already pending. */
  push: (item: T) => void
  /**
   * Synchronously flush any buffered items right now, cancelling the pending
   * animation-frame callback if one was scheduled. No-op when the buffer is
   * empty. Safe to call under fake timers, where `requestAnimationFrame`
   * never fires on its own.
   */
  flush: () => void
  /** Cancel any pending animation-frame callback and discard buffered items without flushing them. */
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
