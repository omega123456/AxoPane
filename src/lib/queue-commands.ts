import type { ConflictResolution, OpSnapshot, StartOpRequest } from '@/lib/types/ipc'
import { invokeCommand } from './ipc/client'

export function startOp(payload: StartOpRequest) {
  return invokeCommand({ command: 'start_op', payload }) as Promise<string>
}

export function pauseOp(id: string) {
  return invokeCommand({ command: 'pause_op', payload: { id } }) as Promise<void>
}

export function resumeOp(id: string) {
  return invokeCommand({ command: 'resume_op', payload: { id } }) as Promise<void>
}

export function cancelOp(id: string) {
  return invokeCommand({ command: 'cancel_op', payload: { id } }) as Promise<void>
}

export function skipOp(id: string) {
  return invokeCommand({ command: 'skip_op', payload: { id } }) as Promise<void>
}

export function retryOp(id: string) {
  return invokeCommand({ command: 'retry_op', payload: { id } }) as Promise<void>
}

export function reorderOps(ids: string[]) {
  return invokeCommand({ command: 'reorder_ops', payload: { ids } }) as Promise<void>
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
  }) as Promise<void>
}

export function queueSnapshot() {
  return invokeCommand({ command: 'queue_snapshot' }) as Promise<OpSnapshot[]>
}

export function hasUnfinishedOps() {
  return invokeCommand({ command: 'has_unfinished_ops' }) as Promise<boolean>
}
