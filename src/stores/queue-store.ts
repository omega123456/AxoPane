import { create } from 'zustand'
import {
  cancelOp,
  pauseOp,
  resolveConflict,
  resumeOp,
  retryOp,
} from '@/lib/queue-commands'
import type {
  ConflictInfo,
  ConflictResolution,
  OpProgress,
  OpSnapshot,
  ThroughputSample,
} from '@/lib/types/ipc'

type QueueStore = {
  /** Operations keyed by id. */
  operations: Record<string, OpProgress>
  /** Submission / display order. */
  order: string[]
  /** Per-bucket averaged throughput samples keyed by operation id. */
  throughputHistory: Record<string, ThroughputSample[]>
  /**
   * The chart's frozen Y-axis maximum: the highest *averaged* (plotted) rate
   * seen so far, non-decreasing for the life of the operation. It ratchets up
   * when the curve reaches a new high and never drops, so the scale holds still
   * (Windows copy-dialog style) as speed falls — no rescaling under the curve.
   */
  throughputPeak: Record<string, number>
  /**
   * The last raw rate folded per operation, used to gate the EMA so a backend
   * value held across chunks doesn't collapse the smoothing. Internal state.
   */
  throughputLastRaw: Record<string, number>
  /** Pending conflicts keyed by operation id. */
  conflicts: Record<string, ConflictInfo>
  /** Whether the queue panel is expanded (vs collapsed toast). */
  expanded: boolean
  setExpanded: (expanded: boolean) => void
  toggleExpanded: () => void
  hydrate: (snapshots: OpSnapshot[]) => void
  applyProgress: (progress: OpProgress) => void
  applyConflict: (conflict: ConflictInfo) => void
  /** Drop an operation the backend has auto-removed from the queue. */
  removeOperation: (id: string) => void
  /** Dismiss a terminal operation from the UI. */
  dismissOperation: (id: string) => void
  /** Side-effecting controls that proxy to the backend and update local state. */
  pause: (id: string) => void
  resume: (id: string) => void
  cancel: (id: string) => void
  retry: (id: string) => void
  resolve: (
    id: string,
    resolution: ConflictResolution,
    applyToAll: boolean,
    renameTo: string | null,
  ) => void
  reset: () => void
}

function defaultState() {
  return {
    operations: {} as Record<string, OpProgress>,
    order: [] as string[],
    throughputHistory: {} as Record<string, ThroughputSample[]>,
    throughputPeak: {} as Record<string, number>,
    throughputLastRaw: {} as Record<string, number>,
    conflicts: {} as Record<string, ConflictInfo>,
    expanded: false,
  }
}

/**
 * Number of progress-% buckets the history is quantised into. A new sample is
 * appended when progress crosses into the next bucket, so history length is
 * bounded and passed buckets stay immutable (preserving the historical shape).
 * Coarser buckets give a cleaner line at the cost of horizontal resolution.
 */
const THROUGHPUT_BUCKETS = 60
/**
 * EMA factor for the continuous smoothed rate. Smoothing carries *across* bucket
 * boundaries so the leading edge never jumps to a raw reading — the whole curve,
 * including the active point, is the same denoised value.
 */
const THROUGHPUT_EMA_FACTOR = 0.25
const TERMINAL: ReadonlySet<OpProgress['status']> = new Set([
  'completed',
  'failed',
  'cancelled',
])

function shouldResetThroughputHistory(
  previousProgress: OpProgress | undefined,
  nextProgress: OpProgress,
): boolean {
  if (!previousProgress) {
    return false
  }

  return (
    nextProgress.progressPercent < previousProgress.progressPercent ||
    nextProgress.copiedBytes < previousProgress.copiedBytes ||
    nextProgress.completedItems < previousProgress.completedItems ||
    nextProgress.status === 'pending'
  )
}

function bucketOf(percent: number): number {
  const clamped = Math.min(100, Math.max(0, percent))
  return Math.min(THROUGHPUT_BUCKETS - 1, Math.floor((clamped / 100) * THROUGHPUT_BUCKETS))
}

type FoldedThroughput = {
  history: ThroughputSample[]
  /** The raw rate just folded, to gate EMA stepping on the next reading. */
  lastRaw: number
}

function foldThroughputSample(
  existing: ThroughputSample[] | undefined,
  lastRaw: number,
  percent: number,
  rate: number,
): FoldedThroughput {
  if (!existing || existing.length === 0) {
    return { history: [{ percent, rate }], lastRaw: rate }
  }

  const lastSample = existing[existing.length - 1]
  // Step the EMA only when the raw rate changes (a new backend sampling window).
  // The backend holds a rate constant between windows and re-emits it on every
  // chunk; stepping per event would let the EMA collapse onto each raw value,
  // re-introducing the jitter. The smoothed value carries across buckets.
  const smoothed =
    rate === lastRaw
      ? lastSample.rate
      : lastSample.rate + (rate - lastSample.rate) * THROUGHPUT_EMA_FACTOR

  // Crossing into a new bucket appends a point (passed buckets stay frozen);
  // otherwise the active point tracks the latest percent + smoothed rate. Either
  // way the value is the *continuous* smoothed rate, so the leading edge is as
  // calm as the rest of the line.
  return bucketOf(percent) > bucketOf(lastSample.percent)
    ? { history: [...existing, { percent, rate: smoothed }], lastRaw: rate }
    : { history: [...existing.slice(0, -1), { percent, rate: smoothed }], lastRaw: rate }
}

function seedThroughputHistory(progress: OpProgress): ThroughputSample[] {
  return [{ percent: progress.progressPercent, rate: progress.bytesPerSecond }]
}

function pruneOperationState(state: QueueStore, id: string): Partial<QueueStore> {
  const operations = { ...state.operations }
  delete operations[id]
  const conflicts = { ...state.conflicts }
  delete conflicts[id]
  const throughputHistory = { ...state.throughputHistory }
  delete throughputHistory[id]
  const throughputPeak = { ...state.throughputPeak }
  delete throughputPeak[id]
  const throughputLastRaw = { ...state.throughputLastRaw }
  delete throughputLastRaw[id]
  return {
    operations,
    conflicts,
    throughputHistory,
    throughputPeak,
    throughputLastRaw,
    order: state.order.filter((entry) => entry !== id),
  }
}

export const useQueueStore = create<QueueStore>((set) => ({
  ...defaultState(),
  setExpanded: (expanded) => set({ expanded }),
  toggleExpanded: () => set((state) => ({ expanded: !state.expanded })),
  hydrate: (snapshots) => {
    set((state) => {
      const operations: Record<string, OpProgress> = {}
      const conflicts: Record<string, ConflictInfo> = {}
      const throughputHistory: Record<string, ThroughputSample[]> = {}
      const throughputPeak: Record<string, number> = {}
      const throughputLastRaw: Record<string, number> = {}
      const order: string[] = []

      for (const snapshot of snapshots) {
        const id = snapshot.progress.operationId
        operations[id] = snapshot.progress
        const existingHistory = state.throughputHistory[id]
        const seededHistory = existingHistory ?? seedThroughputHistory(snapshot.progress)
        throughputHistory[id] = seededHistory
        // Preserve an in-flight EMA gate; seed a fresh one from the snapshot rate.
        throughputLastRaw[id] = existingHistory
          ? state.throughputLastRaw[id] ?? 0
          : snapshot.progress.bytesPerSecond
        throughputPeak[id] = Math.max(
          state.throughputPeak[id] ?? 0,
          seededHistory[seededHistory.length - 1]?.rate ?? 0,
        )
        order.push(id)
        if (snapshot.conflict) {
          conflicts[id] = snapshot.conflict
        }
      }
      return {
        operations,
        order,
        throughputHistory,
        throughputPeak,
        throughputLastRaw,
        conflicts,
      }
    })
  },
  applyProgress: (progress) =>
    set((state) => {
      const previousProgress = state.operations[progress.operationId]
      const resetHistory = shouldResetThroughputHistory(previousProgress, progress)
      const operations = { ...state.operations, [progress.operationId]: progress }
      const folded = resetHistory
        ? { history: seedThroughputHistory(progress), lastRaw: progress.bytesPerSecond }
        : foldThroughputSample(
            state.throughputHistory[progress.operationId],
            state.throughputLastRaw[progress.operationId] ?? 0,
            progress.progressPercent,
            progress.bytesPerSecond,
          )
      const throughputHistory = {
        ...state.throughputHistory,
        [progress.operationId]: folded.history,
      }
      const throughputLastRaw = {
        ...state.throughputLastRaw,
        [progress.operationId]: folded.lastRaw,
      }
      // Frozen Y ceiling: the non-decreasing max of the displayed (averaged)
      // rate. It ratchets up when the curve reaches a new high and never drops,
      // so the scale holds still as speed falls. Reset on rewind/restart.
      const latestRate = folded.history[folded.history.length - 1]?.rate ?? 0
      const throughputPeak = {
        ...state.throughputPeak,
        [progress.operationId]: Math.max(
          resetHistory ? 0 : state.throughputPeak[progress.operationId] ?? 0,
          latestRate,
        ),
      }
      const order = state.order.includes(progress.operationId)
        ? state.order
        : [...state.order, progress.operationId]
      const conflicts = { ...state.conflicts }

      // A completed/auto-removed op stops emitting; we keep it until a later
      // progress event or a hydrate prunes it. Clearing the conflict once the
      // op leaves the conflict state keeps the modal in sync.
      if (progress.status !== 'conflict' && conflicts[progress.operationId]) {
        delete conflicts[progress.operationId]
      }

      return {
        operations,
        order,
        throughputHistory,
        throughputPeak,
        throughputLastRaw,
        conflicts,
      }
    }),
  applyConflict: (conflict) =>
    set((state) => ({
      conflicts: { ...state.conflicts, [conflict.operationId]: conflict },
      order: state.order.includes(conflict.operationId)
        ? state.order
        : [...state.order, conflict.operationId],
      // Surface the panel when a conflict needs attention.
      expanded: true,
    })),
  removeOperation: (id) =>
    set((state) => {
      if (!state.order.includes(id) && !state.operations[id]) {
        return state
      }
      return pruneOperationState(state, id)
    }),
  dismissOperation: (id) =>
    set((state) => {
      const operation = state.operations[id]
      if (!operation || !isTerminal(operation.status)) {
        return state
      }
      return pruneOperationState(state, id)
    }),
  pause: (id) => {
    void pauseOp(id)
    set((state) => updateStatus(state, id, 'paused'))
  },
  resume: (id) => {
    void resumeOp(id)
    set((state) => updateStatus(state, id, 'active'))
  },
  cancel: (id) => {
    void cancelOp(id)
  },
  retry: (id) => {
    void retryOp(id)
    set((state) => {
      const operation = state.operations[id]
      if (!operation) {
        return state
      }
      return {
        operations: {
          ...state.operations,
          [id]: { ...operation, status: 'pending', errorMessage: null },
        },
        throughputHistory: {
          ...state.throughputHistory,
          [id]: [],
        },
        throughputPeak: {
          ...state.throughputPeak,
          [id]: 0,
        },
        throughputLastRaw: {
          ...state.throughputLastRaw,
          [id]: 0,
        },
      }
    })
  },
  resolve: (id, resolution, applyToAll, renameTo) => {
    void resolveConflict(id, resolution, applyToAll, renameTo)
    set((state) => {
      const conflicts = { ...state.conflicts }
      delete conflicts[id]
      return { conflicts }
    })
  },
  reset: () => set(defaultState()),
}))

function updateStatus(
  state: QueueStore,
  id: string,
  status: OpProgress['status'],
): Partial<QueueStore> {
  const operation = state.operations[id]
  if (!operation) {
    return state
  }
  return {
    operations: { ...state.operations, [id]: { ...operation, status } },
  }
}

/** Operations in display order, freshest data resolved from the store map. */
export function orderedOperations(state: QueueStore): OpProgress[] {
  return state.order
    .map((id) => state.operations[id])
    .filter((operation): operation is OpProgress => operation !== undefined)
}

export function activeConflict(state: QueueStore): ConflictInfo | undefined {
  for (const id of state.order) {
    const conflict = state.conflicts[id]
    if (conflict) {
      return conflict
    }
  }
  return undefined
}

export function isTerminal(status: OpProgress['status']): boolean {
  return TERMINAL.has(status)
}

export function hasUnfinishedWork(state: QueueStore): boolean {
  return orderedOperations(state).some((operation) => !isTerminal(operation.status))
}
