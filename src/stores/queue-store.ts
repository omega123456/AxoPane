import { create } from 'zustand'
import { cancelOp, pauseOp, resolveConflict, resumeOp, retryOp, skipOp } from '@/lib/queue-commands'
import type { ConflictInfo, OpProgress, OpSnapshot, ResolveConflictRequest } from '@/lib/types/ipc'
import {
  conflictedQueue,
  defaultQueueData,
  dismissedOperation,
  hydratedQueue,
  isTerminal,
  progressedQueue,
  removedOperation,
  retriedOperation,
  withOperationStatus,
  withoutConflict,
  type QueueData,
} from '@/stores/queue/operations'

export { isTerminal }

type QueueStore = QueueData & {
  setExpanded: (expanded: boolean) => void
  toggleExpanded: () => void
  hydrate: (snapshots: OpSnapshot[]) => void
  applyProgress: (progress: OpProgress) => void
  applyConflict: (conflict: ConflictInfo) => void
  removeOperation: (id: string) => void
  dismissOperation: (id: string) => void
  pause: (id: string) => void
  resume: (id: string) => void
  cancel: (id: string) => void
  skip: (id: string) => void
  retry: (id: string) => void
  resolve: (request: ResolveConflictRequest) => void
  reset: () => void
}

export const useQueueStore = create<QueueStore>((set) => ({
  ...defaultQueueData(),
  setExpanded: (expanded) => set({ expanded }),
  toggleExpanded: () => set((state) => ({ expanded: !state.expanded })),
  hydrate: (snapshots) => set((state) => hydratedQueue(state, snapshots)),
  applyProgress: (progress) => set((state) => progressedQueue(state, progress)),
  applyConflict: (conflict) => set((state) => conflictedQueue(state, conflict)),
  removeOperation: (id) => set((state) => removedOperation(state, id) ?? state),
  dismissOperation: (id) => set((state) => dismissedOperation(state, id) ?? state),
  pause: (id) => {
    void pauseOp(id)
    set((state) => withOperationStatus(state, id, 'paused') ?? state)
  },
  resume: (id) => {
    void resumeOp(id)
    set((state) => withOperationStatus(state, id, 'active') ?? state)
  },
  cancel: (id) => {
    void cancelOp(id)
  },
  skip: (id) => {
    void skipOp(id)
  },
  retry: (id) => {
    void retryOp(id)
    set((state) => retriedOperation(state, id) ?? state)
  },
  resolve: (request) => {
    void resolveConflict(request)
    set((state) => withoutConflict(state, request.id))
  },
  reset: () => set(defaultQueueData()),
}))

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

export function hasUnfinishedWork(state: QueueStore): boolean {
  return orderedOperations(state).some((operation) => !isTerminal(operation.status))
}
