import type { ConflictResolution, StartOpRequest } from '@/lib/types/ipc'
import { invokeCommand } from './ipc/client'

export function startOp(payload: StartOpRequest) {
  return invokeCommand({ command: 'start_op', payload })
}

export function pauseOp(id: string) {
  return invokeCommand({ command: 'pause_op', payload: { id } })
}

export function resumeOp(id: string) {
  return invokeCommand({ command: 'resume_op', payload: { id } })
}

export function cancelOp(id: string) {
  return invokeCommand({ command: 'cancel_op', payload: { id } })
}

export function skipOp(id: string) {
  return invokeCommand({ command: 'skip_op', payload: { id } })
}

export function retryOp(id: string) {
  return invokeCommand({ command: 'retry_op', payload: { id } })
}

export function reorderOps(operationIds: string[]) {
  return invokeCommand({ command: 'reorder_ops', payload: { ids: operationIds } })
}

export function resolveConflict(
  id: string,
  resolution: ConflictResolution,
  applyToAll: boolean,
  renameTo: string | null,
) {
  return invokeCommand({
    command: 'resolve_conflict',
    payload: { id, resolution, applyToAll, renameTo },
  })
}

export function queueSnapshot() {
  return invokeCommand({ command: 'queue_snapshot' })
}

export function hasUnfinishedOps() {
  return invokeCommand({ command: 'has_unfinished_ops' })
}
