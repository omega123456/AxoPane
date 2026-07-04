import { describe, expect, it, vi } from 'vitest'
import { createRafBatcher } from '@/lib/raf-batcher'

/**
 * Replaces `window.requestAnimationFrame`/`cancelAnimationFrame` with a
 * manually-steppable fake so frame delivery is deterministic and instant,
 * matching the "no fixed delays" test-suite constraint.
 */
function installFakeAnimationFrame() {
  let nextId = 0
  const callbacks = new Map<number, FrameRequestCallback>()

  const requestSpy = vi
    .spyOn(window, 'requestAnimationFrame')
    .mockImplementation((callback: FrameRequestCallback) => {
      nextId += 1
      callbacks.set(nextId, callback)
      return nextId
    })
  const cancelSpy = vi
    .spyOn(window, 'cancelAnimationFrame')
    .mockImplementation((handle: number) => {
      callbacks.delete(handle)
    })

  return {
    /** Number of animation-frame callbacks currently scheduled (0 or 1 for a well-behaved batcher). */
    pendingCount: () => callbacks.size,
    /** Runs every currently-scheduled callback, as if one animation frame had elapsed. */
    step(time = 0) {
      const queued = [...callbacks.values()]
      callbacks.clear()
      for (const callback of queued) {
        callback(time)
      }
    },
    restore() {
      callbacks.clear()
      requestSpy.mockRestore()
      cancelSpy.mockRestore()
    },
  }
}

describe('createRafBatcher', () => {
  it('accumulates multiple pushes within a frame and flushes them together once', () => {
    const raf = installFakeAnimationFrame()
    try {
      const onFlush = vi.fn()
      const batcher = createRafBatcher<number>(onFlush)

      batcher.push(1)
      batcher.push(2)
      batcher.push(3)

      // Only one frame callback should be scheduled for the whole burst.
      expect(raf.pendingCount()).toBe(1)
      expect(onFlush).not.toHaveBeenCalled()

      raf.step()

      expect(onFlush).toHaveBeenCalledTimes(1)
      expect(onFlush).toHaveBeenCalledWith([1, 2, 3])
    } finally {
      raf.restore()
    }
  })

  it('schedules a new frame for the next batch after a flush', () => {
    const raf = installFakeAnimationFrame()
    try {
      const onFlush = vi.fn()
      const batcher = createRafBatcher<string>(onFlush)

      batcher.push('a')
      raf.step()
      expect(onFlush).toHaveBeenNthCalledWith(1, ['a'])

      batcher.push('b')
      batcher.push('c')
      raf.step()
      expect(onFlush).toHaveBeenNthCalledWith(2, ['b', 'c'])

      expect(onFlush).toHaveBeenCalledTimes(2)
    } finally {
      raf.restore()
    }
  })

  it('cancel discards buffered items and the pending frame without flushing', () => {
    const raf = installFakeAnimationFrame()
    try {
      const onFlush = vi.fn()
      const batcher = createRafBatcher<number>(onFlush)

      batcher.push(1)
      batcher.push(2)
      expect(raf.pendingCount()).toBe(1)

      batcher.cancel()
      expect(raf.pendingCount()).toBe(0)

      // Stepping after cancel must not invoke the callback (nothing scheduled).
      raf.step()
      expect(onFlush).not.toHaveBeenCalled()

      // A push after cancel starts a fresh, empty buffer.
      batcher.push(9)
      raf.step()
      expect(onFlush).toHaveBeenCalledTimes(1)
      expect(onFlush).toHaveBeenCalledWith([9])
    } finally {
      raf.restore()
    }
  })

  it('flush() synchronously drains the buffer without waiting for a real animation frame', () => {
    // No fake-rAF installed here: this exercises the real
    // window.requestAnimationFrame, proving `flush()` does not depend on the
    // frame callback ever firing (as required under fake timers, where rAF
    // never fires on its own).
    const onFlush = vi.fn()
    const batcher = createRafBatcher<number>(onFlush)

    batcher.push(1)
    batcher.push(2)
    batcher.flush()

    expect(onFlush).toHaveBeenCalledTimes(1)
    expect(onFlush).toHaveBeenCalledWith([1, 2])
  })

  it('flush() is a no-op when the buffer is empty', () => {
    const onFlush = vi.fn()
    const batcher = createRafBatcher<number>(onFlush)

    batcher.flush()

    expect(onFlush).not.toHaveBeenCalled()
  })

  it('flush() cancels a pending scheduled frame so it does not double-flush', () => {
    const raf = installFakeAnimationFrame()
    try {
      const onFlush = vi.fn()
      const batcher = createRafBatcher<number>(onFlush)

      batcher.push(1)
      batcher.flush()
      expect(raf.pendingCount()).toBe(0)
      expect(onFlush).toHaveBeenCalledTimes(1)

      // Stepping the (now-cancelled) frame must not trigger a second flush.
      raf.step()
      expect(onFlush).toHaveBeenCalledTimes(1)
    } finally {
      raf.restore()
    }
  })
})
