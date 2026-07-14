import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { thumbnailFingerprintKey, useThumbnailStore } from '@/stores/thumbnail-store'
import type { ThumbnailCandidateRequest, ThumbnailResultEvent } from '@/lib/types/ipc'

const scope = { paneId: 'left', tabId: 'tab-1', path: 'C:\\files', mode: 'thumbnails' as const }
const candidate: ThumbnailCandidateRequest = {
  path: 'C:\\files\\photo.png',
  modifiedUnixSeconds: 20,
  sizeBytes: 30,
  isDirectory: false,
}

function result(overrides: Partial<ThumbnailResultEvent> = {}): ThumbnailResultEvent {
  return {
    paneId: scope.paneId,
    tabId: scope.tabId,
    path: scope.path,
    generation: 1,
    fingerprintPath: candidate.path,
    modifiedUnixSeconds: candidate.modifiedUnixSeconds,
    sizeBytes: candidate.sizeBytes,
    state: 'ready',
    dataUrl: 'data:image/png;base64,AA==',
    ...overrides,
  }
}

beforeEach(() => {
  useThumbnailStore.getState().reset()
})

describe('thumbnail store', () => {
  it('deduplicates visible candidates, cache hits, and active range reports', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_thumbnails', request)
    await useThumbnailStore.getState().setVisibleCandidates(scope, [candidate, candidate])
    expect(request).toHaveBeenCalledOnce()
    expect(request.mock.calls[0]?.[0].candidates).toEqual([candidate])

    useThumbnailStore.getState().applyThumbnailResults([result()])
    await useThumbnailStore.getState().setVisibleCandidates(scope, [candidate])
    expect(request).toHaveBeenCalledOnce()
    expect(useThumbnailStore.getState().getRecord(candidate)?.dataUrl).toBe(result().dataUrl)
  })

  it('cancels old generations when a range or scope is replaced', async () => {
    const cancel = vi.fn(() => undefined)
    ipc.override('cancel_thumbnails', cancel)
    await useThumbnailStore.getState().setVisibleCandidates(scope, [candidate])
    await useThumbnailStore.getState().setVisibleCandidates(scope, [{ ...candidate, sizeBytes: 31 }])
    expect(cancel).toHaveBeenCalledWith(
      expect.objectContaining({ ...scope, generation: 1 }),
    )
    await useThumbnailStore.getState().cancelScope(scope.paneId, scope.tabId)
    expect(cancel).toHaveBeenCalledWith(
      expect.objectContaining({ ...scope, generation: 2 }),
    )
  })

  it('rejects stale context and fingerprint results without cache mutation', async () => {
    await useThumbnailStore.getState().setVisibleCandidates(scope, [candidate])
    useThumbnailStore.getState().applyThumbnailResults([
      result({ generation: 2 }),
      result({ fingerprintPath: 'C:\\files\\other.png' }),
      result({ path: 'C:\\other' }),
    ])
    expect(useThumbnailStore.getState().cache).toEqual({})
  })

  it('stores failure records as fallback state and suppresses retry until expiry', async () => {
    const request = vi.fn(() => undefined)
    ipc.override('request_thumbnails', request)
    await useThumbnailStore.getState().setVisibleCandidates(scope, [candidate])
    useThumbnailStore.getState().applyThumbnailResults([result({ state: 'unavailable', dataUrl: null })])
    const key = thumbnailFingerprintKey(candidate)
    expect(useThumbnailStore.getState().cache[key]).toMatchObject({ state: 'unavailable', dataUrl: null })
    await useThumbnailStore.getState().setVisibleCandidates({ ...scope, path: 'C:\\files-2' }, [candidate])
    expect(request).toHaveBeenCalledOnce()
  })
})
