import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { useDelayedFlag } from '@/lib/use-delayed-flag'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

it('stays false until the value has been true for the full delay', () => {
  const { result } = renderHook(() => useDelayedFlag(true, 1000))

  expect(result.current).toBe(false)

  act(() => {
    vi.advanceTimersByTime(999)
  })
  expect(result.current).toBe(false)

  act(() => {
    vi.advanceTimersByTime(1)
  })
  expect(result.current).toBe(true)
})

it('never turns true when the value clears before the delay elapses', () => {
  const { result, rerender } = renderHook(({ value }) => useDelayedFlag(value, 1000), {
    initialProps: { value: true },
  })

  act(() => {
    vi.advanceTimersByTime(500)
  })
  rerender({ value: false })

  act(() => {
    vi.advanceTimersByTime(1000)
  })
  expect(result.current).toBe(false)
})

it('turns false immediately when the value clears after the delay', () => {
  const { result, rerender } = renderHook(({ value }) => useDelayedFlag(value, 1000), {
    initialProps: { value: true },
  })

  act(() => {
    vi.advanceTimersByTime(1000)
  })
  expect(result.current).toBe(true)

  rerender({ value: false })
  expect(result.current).toBe(false)
})
