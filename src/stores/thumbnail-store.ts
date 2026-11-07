import { create } from 'zustand'
import { cancelThumbnails, requestThumbnails } from '@/lib/ipc/commands'
import { pathsMatch } from '@/lib/path-compare'
import type { PaneViewMode } from '@/lib/pane-view'
import type { ThumbnailCandidateRequest, ThumbnailResultEvent } from '@/lib/types/ipc'
import {
  isThumbnailCacheRecordUsable,
  pruneThumbnailCache,
  type ThumbnailCacheRecord,
} from '@/stores/thumbnails/cache-policy'
import { cacheNow } from '@/stores/weighted-cache'
import { utf16ByteWeight } from '@/lib/numbers'

export type ThumbnailScope = {
  paneId: string
  tabId: string
  path: string
  generation: number
  mode: PaneViewMode
}

type ActiveThumbnailScope = ThumbnailScope & {
  revision: number
  candidates: Record<string, ThumbnailCandidateRequest>
  candidateSignature: string
}

type ThumbnailStore = {
  scopes: Record<string, ActiveThumbnailScope>
  cache: Record<string, ThumbnailCacheRecord>
  setVisibleCandidates: (
    scope: Omit<ThumbnailScope, 'generation'>,
    candidates: ThumbnailCandidateRequest[],
  ) => Promise<void>
  cancelScope: (paneId: string, tabId: string) => Promise<void>
  applyThumbnailResults: (events: ThumbnailResultEvent[]) => void
  getRecord: (
    candidate: Pick<ThumbnailCandidateRequest, 'path' | 'modifiedUnixSeconds' | 'sizeBytes'>,
  ) => ThumbnailCacheRecord | undefined
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
  return Array.from(
    new Set(
      candidates
        .filter((candidate) => !candidate.isDirectory)
        .map(
          (candidate) =>
            `${thumbnailFingerprintKey(candidate)}\u0000${candidate.priority}\u0000${candidate.order}`,
        ),
    ),
  )
    .sort()
    .join('\u0001')
}

function visibleProtection(scopes: Record<string, ActiveThumbnailScope>) {
  return new Set(
    Object.values(scopes).flatMap((scope) =>
      scope.mode === 'thumbnails'
        ? Object.entries(scope.candidates).flatMap(([key, candidate]) =>
            candidate.priority === 'visible' ? [key] : [],
          )
        : [],
    ),
  )
}

function nextGeneration(previous?: ActiveThumbnailScope) {
  return (previous?.generation ?? 0) + 1
}

function sameScopeContext(
  previous: ActiveThumbnailScope | undefined,
  scope: Omit<ThumbnailScope, 'generation'>,
): previous is ActiveThumbnailScope {
  return (
    previous !== undefined && previous.mode === scope.mode && pathsMatch(previous.path, scope.path)
  )
}

function nextScopeVersion(
  previous: ActiveThumbnailScope | undefined,
  scope: Omit<ThumbnailScope, 'generation'>,
) {
  if (sameScopeContext(previous, scope)) {
    return { generation: previous.generation, revision: previous.revision + 1 }
  }
  return { generation: nextGeneration(previous), revision: 1 }
}

function uniqueFileCandidates(candidates: ThumbnailCandidateRequest[]) {
  return Object.values(
    Object.fromEntries(
      candidates
        .filter((candidate) => !candidate.isDirectory)
        .map((candidate) => [thumbnailFingerprintKey(candidate), candidate]),
    ),
  )
}

function touchCacheRecords(
  cache: Record<string, ThumbnailCacheRecord>,
  fingerprints: string[],
  now: number,
) {
  const touched = { ...cache }
  for (const fingerprint of fingerprints) {
    const record = touched[fingerprint]
    if (record) touched[fingerprint] = { ...record, touched: now }
  }
  return touched
}

function needsThumbnail(record: ThumbnailCacheRecord | undefined, now: number) {
  if (!record) {
    return true
  }
  if (!isThumbnailCacheRecordUsable(record, now)) {
    return true
  }
  const isLowQualityPreview = record.state === 'ready' && record.quality !== 'high'
  return isLowQualityPreview
}

type FailedThumbnailRequest = {
  key: string
  generation: number
  revision: number
  missing: ThumbnailCandidateRequest[]
  failedAt: number
}

function isCurrentRequest(
  scope: ActiveThumbnailScope | undefined,
  generation: number,
  revision: number,
) {
  if (!scope) {
    return false
  }
  const sameGeneration = scope.generation === generation
  return sameGeneration && scope.revision === revision
}

function markRequestFailed(
  state: ThumbnailStore,
  { key, generation, revision, missing, failedAt }: FailedThumbnailRequest,
) {
  if (!isCurrentRequest(state.scopes[key], generation, revision)) {
    return state
  }
  const nextCache = { ...state.cache }
  for (const candidate of missing) {
    nextCache[thumbnailFingerprintKey(candidate)] = {
      state: 'failed',
      quality: null,
      dataUrl: null,
      touched: failedAt,
      weight: 1,
    }
  }
  return { cache: pruneThumbnailCache(nextCache, visibleProtection(state.scopes), failedAt) }
}

function resultFingerprint(event: ThumbnailResultEvent) {
  return thumbnailFingerprintKey({
    path: event.fingerprintPath,
    modifiedUnixSeconds: event.modifiedUnixSeconds,
    sizeBytes: event.sizeBytes,
  })
}

function scopeAcceptsResult(
  scope: ActiveThumbnailScope | undefined,
  event: ThumbnailResultEvent,
  fingerprint: string,
) {
  if (!scope) {
    return false
  }
  if (scope.mode !== 'thumbnails') {
    return false
  }
  if (scope.generation !== event.generation) {
    return false
  }
  if (!pathsMatch(scope.path, event.path)) {
    return false
  }
  return Boolean(scope.candidates[fingerprint])
}

function resultCacheRecord(event: ThumbnailResultEvent, now: number): ThumbnailCacheRecord {
  const ready = event.state === 'ready'
  return {
    state: event.state,
    quality: ready ? event.quality : null,
    dataUrl: ready ? event.dataUrl : null,
    touched: now,
    weight: utf16ByteWeight(ready ? event.dataUrl : null),
  }
}

export const useThumbnailStore = create<ThumbnailStore>((set, get) => ({
  scopes: {},
  cache: {},
  setVisibleCandidates: async (scope, candidates) => {
    const key = scopeKey(scope.paneId, scope.tabId)
    const signature = candidateSignature(candidates)
    const previous = get().scopes[key]
    const contextMatches = sameScopeContext(previous, scope)
    const candidatesUnchanged = contextMatches && previous.candidateSignature === signature
    if (candidatesUnchanged) return

    const contextChanged = previous !== undefined && !contextMatches
    const cancelPrevious = contextChanged ? cancelThumbnails(previous) : undefined
    const { generation, revision } = nextScopeVersion(previous, scope)
    const visibleCandidates = uniqueFileCandidates(candidates)
    const candidatesByFingerprint = Object.fromEntries(
      visibleCandidates.map((candidate) => [thumbnailFingerprintKey(candidate), candidate]),
    )
    const nextScope: ActiveThumbnailScope = {
      ...scope,
      generation,
      revision,
      candidates: candidatesByFingerprint,
      candidateSignature: signature,
    }
    const scopes = { ...get().scopes, [key]: nextScope }
    const now = cacheNow()
    const touchedCache = touchCacheRecords(get().cache, Object.keys(candidatesByFingerprint), now)
    const cache = pruneThumbnailCache(touchedCache, visibleProtection(scopes), now)
    set({ scopes, cache })

    if (scope.mode !== 'thumbnails') {
      await cancelPrevious
      return
    }
    const missing = visibleCandidates.filter((candidate) =>
      needsThumbnail(cache[thumbnailFingerprintKey(candidate)], now),
    )

    try {
      await cancelPrevious
      const response = await requestThumbnails({
        ...scope,
        generation,
        revision,
        candidates: missing,
      })
      if (response.revision !== revision || response.acceptedCount !== missing.length) {
        throw new Error('thumbnail request was not fully accepted')
      }
    } catch (error) {
      const failedAt = cacheNow()
      set((state) => markRequestFailed(state, { key, generation, revision, missing, failedAt }))
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
      return {
        scopes,
        cache: pruneThumbnailCache(state.cache, visibleProtection(scopes), cacheNow()),
      }
    })
  },
  applyThumbnailResults: (events) =>
    set((state) => {
      if (events.length === 0) return state
      const now = cacheNow()
      const cache = { ...state.cache }
      let changed = false
      for (const event of events) {
        const scope = state.scopes[scopeKey(event.paneId, event.tabId)]
        const fingerprint = resultFingerprint(event)
        if (!scopeAcceptsResult(scope, event, fingerprint)) {
          continue
        }
        cache[fingerprint] = resultCacheRecord(event, now)
        changed = true
      }
      if (!changed) return state
      return { cache: pruneThumbnailCache(cache, visibleProtection(state.scopes), now) }
    }),
  getRecord: (candidate) => {
    const record = get().cache[thumbnailFingerprintKey(candidate)]
    return record && isThumbnailCacheRecordUsable(record, cacheNow()) ? record : undefined
  },
  reset: () => set({ scopes: {}, cache: {} }),
}))
