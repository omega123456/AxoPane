import { create, type StoreApi } from 'zustand'
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

function fileCandidates(candidates: ThumbnailCandidateRequest[]) {
  return candidates.filter((candidate) => !candidate.isDirectory)
}

function candidateSignatureEntry(candidate: ThumbnailCandidateRequest) {
  return `${thumbnailFingerprintKey(candidate)}\u0000${candidate.priority}\u0000${candidate.order}`
}

function candidateSignature(candidates: ThumbnailCandidateRequest[]) {
  const signatureEntries = fileCandidates(candidates).map(candidateSignatureEntry)
  const uniqueSignatureEntries = Array.from(new Set(signatureEntries))
  return uniqueSignatureEntries.sort().join('\u0001')
}

function visibleCandidateKeys(scope: ActiveThumbnailScope) {
  if (scope.mode !== 'thumbnails') {
    return []
  }
  return Object.keys(scope.candidates).filter((key) => scope.candidates[key].priority === 'visible')
}

function visibleProtection(scopes: Record<string, ActiveThumbnailScope>) {
  const visibleKeys = Object.values(scopes).flatMap(visibleCandidateKeys)
  return new Set(visibleKeys)
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
  const candidatesByFingerprint = new Map<string, ThumbnailCandidateRequest>()
  for (const candidate of fileCandidates(candidates)) {
    candidatesByFingerprint.set(thumbnailFingerprintKey(candidate), candidate)
  }
  return Array.from(candidatesByFingerprint.values())
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

type ThumbnailData = Pick<ThumbnailStore, 'scopes' | 'cache'>

function withoutScope(state: ThumbnailData, key: string, now: number): ThumbnailData {
  const scopes = { ...state.scopes }
  delete scopes[key]
  return { scopes, cache: pruneThumbnailCache(state.cache, visibleProtection(scopes), now) }
}

function acceptedResults(state: ThumbnailData, events: ThumbnailResultEvent[]) {
  return events.filter((event) => {
    const scope = state.scopes[scopeKey(event.paneId, event.tabId)]
    return scopeAcceptsResult(scope, event, resultFingerprint(event))
  })
}

function withThumbnailResults(
  state: ThumbnailData,
  events: ThumbnailResultEvent[],
  now: number,
): Pick<ThumbnailData, 'cache'> | null {
  const accepted = acceptedResults(state, events)
  if (accepted.length === 0) {
    return null
  }
  const cache = { ...state.cache }
  for (const event of accepted) {
    cache[resultFingerprint(event)] = resultCacheRecord(event, now)
  }
  return { cache: pruneThumbnailCache(cache, visibleProtection(state.scopes), now) }
}

type ThumbnailSet = StoreApi<ThumbnailStore>['setState']
type ThumbnailGet = () => ThumbnailStore
type VisibleScope = Parameters<ThumbnailStore['setVisibleCandidates']>[0]
type VisibleCandidates = Parameters<ThumbnailStore['setVisibleCandidates']>[1]

function nextActiveScope(
  previous: ActiveThumbnailScope | undefined,
  scope: VisibleScope,
  visibleCandidates: ReturnType<typeof uniqueFileCandidates>,
  signature: string,
): ActiveThumbnailScope {
  const { generation, revision } = nextScopeVersion(previous, scope)
  return {
    ...scope,
    generation,
    revision,
    candidates: Object.fromEntries(
      visibleCandidates.map((candidate) => [thumbnailFingerprintKey(candidate), candidate]),
    ),
    candidateSignature: signature,
  }
}

async function setVisibleCandidates(
  set: ThumbnailSet,
  get: ThumbnailGet,
  scope: VisibleScope,
  candidates: VisibleCandidates,
) {
  const key = scopeKey(scope.paneId, scope.tabId)
  const signature = candidateSignature(candidates)
  const previous = get().scopes[key]
  const contextMatches = sameScopeContext(previous, scope)
  const candidatesUnchanged = contextMatches && previous.candidateSignature === signature
  if (candidatesUnchanged) return

  const contextChanged = previous !== undefined && !contextMatches
  const cancelPrevious = contextChanged ? cancelThumbnails(previous) : undefined
  const visibleCandidates = uniqueFileCandidates(candidates)
  const nextScope = nextActiveScope(previous, scope, visibleCandidates, signature)
  const scopes = { ...get().scopes, [key]: nextScope }
  const now = cacheNow()
  const touchedCache = touchCacheRecords(get().cache, Object.keys(nextScope.candidates), now)
  const cache = pruneThumbnailCache(touchedCache, visibleProtection(scopes), now)
  set({ scopes, cache })

  if (scope.mode !== 'thumbnails') {
    await cancelPrevious
    return
  }
  const missing = visibleCandidates.filter((candidate) =>
    needsThumbnail(cache[thumbnailFingerprintKey(candidate)], now),
  )
  const { generation, revision } = nextScope
  await requestMissingThumbnails(set, { key, scope, generation, revision, missing, cancelPrevious })
}

type MissingThumbnailRequest = {
  key: string
  scope: VisibleScope
  generation: number
  revision: number
  missing: VisibleCandidates
  cancelPrevious: Promise<unknown> | undefined
}

async function requestMissingThumbnails(set: ThumbnailSet, request: MissingThumbnailRequest) {
  const { key, scope, generation, revision, missing, cancelPrevious } = request
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
}

export const useThumbnailStore = create<ThumbnailStore>((set, get) => ({
  scopes: {},
  cache: {},
  setVisibleCandidates: (scope, candidates) => setVisibleCandidates(set, get, scope, candidates),
  cancelScope: async (paneId, tabId) => {
    const key = scopeKey(paneId, tabId)
    const previous = get().scopes[key]
    if (!previous) return
    await cancelThumbnails(previous)
    set((state) => withoutScope(state, key, cacheNow()))
  },
  applyThumbnailResults: (events) => {
    if (events.length === 0) return
    const now = cacheNow()
    set((state) => withThumbnailResults(state, events, now) ?? state)
  },
  getRecord: (candidate) => {
    const record = get().cache[thumbnailFingerprintKey(candidate)]
    return record && isThumbnailCacheRecordUsable(record, cacheNow()) ? record : undefined
  },
  reset: () => set({ scopes: {}, cache: {} }),
}))
