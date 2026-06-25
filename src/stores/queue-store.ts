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
} from '@/lib/types/ipc'

type QueueStore = {
  /** Operations keyed by id. */
  operations: Record<string, OpProgress>
  /** Submission / display order. */
  order: string[]
  /** Recent throughput samples keyed by operation id. */
  throughputHistory: Record<string, number[]>
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
    throughputHistory: {} as Record<string, number[]>,
    conflicts: {} as Record<string, ConflictInfo>,
    expanded: false,
  }
}

const MAX_THROUGHPUT_SAMPLES = 16
const TERMINAL: ReadonlySet<OpProgress['status']> = new Set([
  'completed',
  'failed',
  'cancelled',
])

function appendThroughputSample(existing: number[] | undefined, sample: number): number[] {
  const next = [...(existing ?? []), sample]
  return next.slice(-MAX_THROUGHPUT_SAMPLES)
}

function pruneOperationState(state: QueueStore, id: string): Partial<QueueStore> {
  const operations = { ...state.operations }
  delete operations[id]
  const conflicts = { ...state.conflicts }
  delete conflicts[id]
  const throughputHistory = { ...state.throughputHistory }
  delete throughputHistory[id]
  return {
    operations,
    conflicts,
    throughputHistory,
    order: state.order.filter((entry) => entry !== id),
  }
}

export const useQueueStore = create<QueueStore>((set) => ({
  ...defaultState(),
  setExpanded: (expanded) => set({ expanded }),
  toggleExpanded: () => set((state) => ({ expanded: !state.expanded })),
  hydrate: (snapshots) => {
    const operations: Record<string, OpProgress> = {}
    const conflicts: Record<string, ConflictInfo> = {}
    const throughputHistory: Record<string, number[]> = {}
    const order: string[] = []
    for (const snapshot of snapshots) {
      operations[snapshot.progress.operationId] = snapshot.progress
      throughputHistory[snapshot.progress.operationId] = appendThroughputSample(
        undefined,
        snapshot.progress.bytesPerSecond,
      )
      order.push(snapshot.progress.operationId)
      if (snapshot.conflict) {
        conflicts[snapshot.progress.operationId] = snapshot.conflict
      }
    }
    set({ operations, order, throughputHistory, conflicts })
  },
  applyProgress: (progress) =>
    set((state) => {
      const operations = { ...state.operations, [progress.operationId]: progress }
      const throughputHistory = {
        ...state.throughputHistory,
        [progress.operationId]: appendThroughputSample(
          state.throughputHistory[progress.operationId],
          progress.bytesPerSecond,
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

      return { operations, order, throughputHistory, conflicts }
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
