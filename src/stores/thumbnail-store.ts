import { create } from 'zustand'
import { cancelThumbnails, requestThumbnails } from '@/lib/ipc/commands'
import { pathsMatch } from '@/lib/path-compare'
import type { PaneViewMode } from '@/lib/pane-view'
import type { ThumbnailCandidateRequest, ThumbnailResultEvent } from '@/lib/types/ipc'
import {
  isThumbnailCacheRecordUsable,
  pruneThumbnailCache,
  thumbnailCacheNow,
  thumbnailWeight,
  type ThumbnailCacheRecord,
} from '@/stores/thumbnails/cache-policy'

export type ThumbnailScope = {
  paneId: string
  tabId: string
  path: string
  generation: number
  mode: PaneViewMode
}

type ActiveThumbnailScope = ThumbnailScope & {
  candidates: Record<string, ThumbnailCandidateRequest>
  candidateSignature: string
}

type ThumbnailStore = {
  scopes: Record<string, ActiveThumbnailScope>
  cache: Record<string, ThumbnailCacheRecord>
  beginScope: (scope: Omit<ThumbnailScope, 'generation'>) => number
  setVisibleCandidates: (
    scope: Omit<ThumbnailScope, 'generation'>,
    candidates: ThumbnailCandidateRequest[],
  ) => Promise<void>
  cancelScope: (paneId: string, tabId: string) => Promise<void>
  applyThumbnailResults: (events: ThumbnailResultEvent[]) => void
  getRecord: (candidate: Pick<ThumbnailCandidateRequest, 'path' | 'modifiedUnixSeconds' | 'sizeBytes'>) =>
    | ThumbnailCacheRecord
    | undefined
  reset: () => void
}

function scopeKey(paneId: string, tabId: string) {
  return `${paneId}\u0000${tabId}`
}

export function thumbnailFingerprintKey(
  candidate: Pick<ThumbnailCandidateRequest, 'path' | 'modifiedUnixSeconds' | 'sizeBytes'>,
) {
  return `${candidate.path}\u0000${candidate.modifiedUnixSeconds}\u0000${candidate.sizeBytes}`
}

function candidateSignature(candidates: ThumbnailCandidateRequest[]) {
  return candidates
    .filter((candidate) => !candidate.isDirectory)
    .map(thumbnailFingerprintKey)
    .sort()
    .join('\u0001')
}

function visibleProtection(scopes: Record<string, ActiveThumbnailScope>) {
  return new Set(
    Object.values(scopes).flatMap((scope) =>
      scope.mode === 'thumbnails' ? Object.keys(scope.candidates) : [],
    ),
  )
}

function nextGeneration(previous?: ActiveThumbnailScope) {
  return (previous?.generation ?? 0) + 1
}

export const useThumbnailStore = create<ThumbnailStore>((set, get) => ({
  scopes: {},
  cache: {},
  beginScope: (scope) => {
    const key = scopeKey(scope.paneId, scope.tabId)
    const previous = get().scopes[key]
    if (previous) {
      void cancelThumbnails(previous)
    }
    const generation = nextGeneration(previous)
    set((state) => ({
      scopes: {
        ...state.scopes,
        [key]: { ...scope, generation, candidates: {}, candidateSignature: '' },
      },
    }))
    return generation
  },
  setVisibleCandidates: async (scope, candidates) => {
    const key = scopeKey(scope.paneId, scope.tabId)
    const signature = candidateSignature(candidates)
    const previous = get().scopes[key]
    const contextMatches =
      previous &&
      previous.mode === scope.mode &&
      pathsMatch(previous.path, scope.path) &&
      previous.candidateSignature === signature
    if (contextMatches) return

    if (previous) {
      await cancelThumbnails(previous)
    }

    const generation = nextGeneration(previous)
    const visibleCandidates = Object.values(
      Object.fromEntries(
        candidates
          .filter((candidate) => !candidate.isDirectory)
          .map((candidate) => [thumbnailFingerprintKey(candidate), candidate]),
      ),
    )
    const candidatesByFingerprint = Object.fromEntries(
      visibleCandidates.map((candidate) => [thumbnailFingerprintKey(candidate), candidate]),
    )
    const nextScope: ActiveThumbnailScope = {
      ...scope,
      generation,
      candidates: candidatesByFingerprint,
      candidateSignature: signature,
    }
    const scopes = { ...get().scopes, [key]: nextScope }
    const now = thumbnailCacheNow()
    const cache = pruneThumbnailCache(get().cache, visibleProtection(scopes), now)
    set({ scopes, cache })

    if (scope.mode !== 'thumbnails') return
    const missing = visibleCandidates.filter((candidate) => {
      const record = cache[thumbnailFingerprintKey(candidate)]
      return !record || !isThumbnailCacheRecordUsable(record, now)
    })
    if (missing.length === 0) return

    try {
      await requestThumbnails({ ...scope, generation, candidates: missing })
    } catch (error) {
      const failedAt = thumbnailCacheNow()
      set((state) => {
        const current = state.scopes[key]
        if (!current || current.generation !== generation) return state
        const nextCache = { ...state.cache }
        for (const candidate of missing) {
          const fingerprint = thumbnailFingerprintKey(candidate)
          nextCache[fingerprint] = { state: 'failed', dataUrl: null, touched: failedAt, weight: 1 }
        }
        return { cache: pruneThumbnailCache(nextCache, visibleProtection(state.scopes), failedAt) }
      })
      throw error
    }
  },
  cancelScope: async (paneId, tabId) => {
    const key = scopeKey(paneId, tabId)
    const previous = get().scopes[key]
    if (!previous) return
    await cancelThumbnails(previous)
    set((state) => {
      const scopes = { ...state.scopes }
      delete scopes[key]
      return { scopes, cache: pruneThumbnailCache(state.cache, visibleProtection(scopes), thumbnailCacheNow()) }
    })
  },
  applyThumbnailResults: (events) =>
    set((state) => {
      if (events.length === 0) return state
      const now = thumbnailCacheNow()
      const cache = { ...state.cache }
      let changed = false
      for (const event of events) {
        const scope = state.scopes[scopeKey(event.paneId, event.tabId)]
        const fingerprint = thumbnailFingerprintKey({
          path: event.fingerprintPath,
          modifiedUnixSeconds: event.modifiedUnixSeconds,
          sizeBytes: event.sizeBytes,
        })
        if (
          !scope ||
          scope.mode !== 'thumbnails' ||
          scope.generation !== event.generation ||
          !pathsMatch(scope.path, event.path) ||
          !scope.candidates[fingerprint]
        ) {
          continue
        }
        cache[fingerprint] = {
          state: event.state,
          dataUrl: event.state === 'ready' ? event.dataUrl : null,
          touched: now,
          weight: thumbnailWeight(event.state === 'ready' ? event.dataUrl : null),
        }
        changed = true
      }
      if (!changed) return state
      return { cache: pruneThumbnailCache(cache, visibleProtection(state.scopes), now) }
    }),
  getRecord: (candidate) => {
    const record = get().cache[thumbnailFingerprintKey(candidate)]
    return record && isThumbnailCacheRecordUsable(record, thumbnailCacheNow()) ? record : undefined
  },
  reset: () => set({ scopes: {}, cache: {} }),
}))
