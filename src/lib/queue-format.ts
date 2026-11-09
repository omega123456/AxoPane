import { formatBytes, formatCount, formatEta } from '@/lib/format'
import type { OpKind, OpProgress, OpStatus } from '@/lib/types/ipc'
import { isTerminal } from '@/stores/queue-store'

export const ITEM_PREVIEW_LIMIT = 2

export const operationVerbs: Record<OpKind, string> = {
  copy: 'Copying',
  move: 'Moving',
  delete: 'Deleting',
  compress: 'Compressing',
  extract: 'Extracting',
}

export function verb(operation: OpProgress) {
  return operationVerbs[operation.kind]
}

const finishedTitleSuffixes: Partial<Record<OpStatus, string>> = {
  completed: 'complete',
  failed: 'failed',
  cancelled: 'cancelled',
}

export function formatJobTitle(operation: OpProgress) {
  const suffix =
    finishedTitleSuffixes[operation.status] ?? `${formatCount(operation.totalItems)} items`
  return `${verb(operation)} ${suffix}`
}

export type JobFlags = {
  isCompleted: boolean
  isFailed: boolean
  isPaused: boolean
  isCancelled: boolean
  isConflict: boolean
  isPending: boolean
  isFinished: boolean
  isUnderway: boolean
  showsTransferRate: boolean
}

export function jobFlags(operation: OpProgress, hasConflict: boolean): JobFlags {
  const status = operation.status
  const isFinished = isTerminal(status)
  const isPending = status === 'pending'
  const isUnderway = !isPending && !isFinished
  return {
    isCompleted: status === 'completed',
    isFailed: status === 'failed',
    isPaused: status === 'paused',
    isCancelled: status === 'cancelled',
    isConflict: status === 'conflict' || hasConflict,
    isPending,
    isFinished,
    isUnderway,
    showsTransferRate: isUnderway && operation.kind !== 'delete',
  }
}

export function formatItemPreview(itemNames: string[], totalItems: number): string | null {
  if (itemNames.length === 0) {
    return null
  }
  if (totalItems <= ITEM_PREVIEW_LIMIT) {
    return itemNames.join(', ')
  }
  return `${itemNames.slice(0, ITEM_PREVIEW_LIMIT).join(', ')}, +${formatCount(
    totalItems - ITEM_PREVIEW_LIMIT,
  )} more`
}

export function formatJobProgress(operation: OpProgress, options: { scanning: boolean }): string[] {
  const measured =
    operation.kind === 'delete'
      ? `${formatCount(
          Math.min(operation.completedItems, operation.totalItems),
        )} of ${formatCount(operation.totalItems)} items deleted`
      : `${formatBytes(operation.copiedBytes)} of ${formatBytes(operation.totalBytes)}`

  const totalStillGrowing = options.scanning && operation.kind !== 'delete'
  if (totalStillGrowing) {
    return [`${measured} (still scanning)`]
  }
  if (operation.status === 'paused') {
    return [measured, 'paused']
  }
  if (operation.etaSeconds === null) {
    return [measured, 'estimating…']
  }
  return [measured, formatEta(operation.etaSeconds)]
}
