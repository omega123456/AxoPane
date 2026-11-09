import type { ConflictInfo, OpProgress, OpSnapshot } from '@/lib/types/ipc'
import { withoutKey } from '@/lib/collections'
import {
  CLEARED_THROUGHPUT_TRACK,
  foldedThroughputTrack,
  freshThroughputTrack,
  hydratedThroughputTrack,
  restartsThroughput,
  type ThroughputTrack,
} from '@/stores/queue/throughput'

export type QueueData = {
  operations: Record<string, OpProgress>
  order: string[]
  throughput: Record<string, ThroughputTrack>
  conflicts: Record<string, ConflictInfo>
  expanded: boolean
}

type QueueEntries = Omit<QueueData, 'expanded'>

const TERMINAL: ReadonlySet<OpProgress['status']> = new Set(['completed', 'failed', 'cancelled'])

export function isTerminal(status: OpProgress['status']): boolean {
  return TERMINAL.has(status)
}

function emptyQueueEntries(): QueueEntries {
  return { operations: {}, order: [], throughput: {}, conflicts: {} }
}

export function defaultQueueData(): QueueData {
  return { ...emptyQueueEntries(), expanded: false }
}

function withOperationId(order: string[], operationId: string) {
  return order.includes(operationId) ? order : [...order, operationId]
}

function progressedTrack(state: QueueData, progress: OpProgress): ThroughputTrack {
  const previous = state.operations[progress.operationId]
  if (restartsThroughput(previous, progress)) {
    return freshThroughputTrack(progress)
  }
  return foldedThroughputTrack(state.throughput[progress.operationId], progress)
}

function conflictsAfterProgress(conflicts: Record<string, ConflictInfo>, progress: OpProgress) {
  if (progress.status === 'conflict') {
    return { ...conflicts }
  }
  return withoutKey(conflicts, progress.operationId)
}

export function hydratedQueue(state: QueueData, snapshots: OpSnapshot[]): QueueEntries {
  const hydrated = emptyQueueEntries()
  for (const { progress, conflict } of snapshots) {
    const id = progress.operationId
    hydrated.operations[id] = progress
    hydrated.throughput[id] = hydratedThroughputTrack(state.throughput[id], progress)
    hydrated.order.push(id)
    if (conflict) {
      hydrated.conflicts[id] = conflict
    }
  }
  return hydrated
}

export function progressedQueue(state: QueueData, progress: OpProgress): QueueEntries {
  const id = progress.operationId
  return {
    operations: { ...state.operations, [id]: progress },
    order: withOperationId(state.order, id),
    throughput: { ...state.throughput, [id]: progressedTrack(state, progress) },
    conflicts: conflictsAfterProgress(state.conflicts, progress),
  }
}

export function conflictedQueue(state: QueueData, conflict: ConflictInfo): Partial<QueueData> {
  return {
    conflicts: { ...state.conflicts, [conflict.operationId]: conflict },
    order: withOperationId(state.order, conflict.operationId),
    expanded: true,
  }
}

function prunedQueue(state: QueueData, id: string): QueueEntries {
  return {
    operations: withoutKey(state.operations, id),
    conflicts: withoutKey(state.conflicts, id),
    throughput: withoutKey(state.throughput, id),
    order: state.order.filter((entry) => entry !== id),
  }
}

function isKnownOperation(state: QueueData, id: string) {
  return state.order.includes(id) || Boolean(state.operations[id])
}

function isDismissable(state: QueueData, id: string) {
  const operation = state.operations[id]
  return operation !== undefined && isTerminal(operation.status)
}

export function removedOperation(state: QueueData, id: string): QueueEntries | null {
  return isKnownOperation(state, id) ? prunedQueue(state, id) : null
}

export function dismissedOperation(state: QueueData, id: string): QueueEntries | null {
  return isDismissable(state, id) ? prunedQueue(state, id) : null
}

export function withOperationStatus(
  state: QueueData,
  id: string,
  status: OpProgress['status'],
): Partial<QueueData> | null {
  const operation = state.operations[id]
  if (!operation) {
    return null
  }
  return { operations: { ...state.operations, [id]: { ...operation, status } } }
}

export function retriedOperation(state: QueueData, id: string): Partial<QueueData> | null {
  const operation = state.operations[id]
  if (!operation) {
    return null
  }
  return {
    operations: {
      ...state.operations,
      [id]: { ...operation, status: 'pending', errorMessage: null },
    },
    throughput: { ...state.throughput, [id]: CLEARED_THROUGHPUT_TRACK },
  }
}

export function withoutConflict(state: QueueData, id: string): Partial<QueueData> {
  return { conflicts: withoutKey(state.conflicts, id) }
}

export function withReorderedPending(
  state: Pick<QueueData, 'order'>,
  pendingIds: string[],
  reorderedPendingIds: string[],
): Pick<QueueData, 'order'> {
  const nonPending = state.order.filter((operationId) => !pendingIds.includes(operationId))
  return { order: [...nonPending, ...reorderedPendingIds] }
}
