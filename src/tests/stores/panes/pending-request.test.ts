import { describe, expect, it, vi } from 'vitest'
import { runMarkingPending } from '@/stores/panes/pending-request'

describe('runMarkingPending', () => {
  it('marks pending before the request and leaves it marked on success', async () => {
    const markPending = vi.fn()
    const request = vi.fn(async () => {
      expect(markPending).toHaveBeenCalledWith(true)
    })

    await runMarkingPending({ markPending, request })

    expect(request).toHaveBeenCalledOnce()
    expect(markPending.mock.calls).toEqual([[true]])
  })

  it('clears the pending mark and rethrows when the request fails', async () => {
    const markPending = vi.fn()
    const failure = new Error('request failed')

    await expect(
      runMarkingPending({ markPending, request: () => Promise.reject(failure) }),
    ).rejects.toBe(failure)

    expect(markPending.mock.calls).toEqual([[true], [false]])
  })
})
