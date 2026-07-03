import { useMemo } from 'react'
import {
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  GripVerticalIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  XCircleIcon,
  XIcon,
} from '@/components/icons'
import { JobCard } from '@/components/queue/JobCard'
import { reorderOps } from '@/lib/queue-commands'
import { formatCount } from '@/lib/format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'
import { isTerminal, useQueueStore } from '@/stores/queue-store'

/** Move a pending operation within the pending sub-list and persist the order. */
function reorder(operations: OpProgress[], id: string, delta: number) {
  const pending = operations
    .filter((operation) => operation.status === 'pending')
    .map((operation) => operation.operationId)
  const index = pending.indexOf(id)
  const target = index + delta
  if (index < 0 || target < 0 || target >= pending.length) {
    return
  }
  const next = [...pending]
  ;[next[index], next[target]] = [next[target], next[index]]
  void reorderOps(next)
  useQueueStore.setState((state) => {
    const nonPending = state.order.filter(
      (operationId) => !pending.includes(operationId),
    )
    return { order: [...nonPending, ...next] }
  })
}

function jobPriority(operation: OpProgress, hasConflict: boolean) {
  if (hasConflict || operation.status === 'conflict') {
    return 0
  }
  if (operation.status === 'active' || operation.status === 'paused') {
    return 1
  }
  if (operation.status === 'pending') {
    return 2
  }
  if (operation.status === 'failed') {
    return 3
  }
  if (operation.status === 'cancelled') {
    return 4
  }
  return 5
}

function primaryOperation(operations: OpProgress[], conflicts: Record<string, unknown>) {
  return operations
    .map((operation, index) => ({
      operation,
      index,
      priority: jobPriority(operation, Boolean(conflicts[operation.operationId])),
    }))
    .sort((left, right) => left.priority - right.priority || left.index - right.index)[0]
    ?.operation
}

function verb(operation: OpProgress) {
  if (operation.kind === 'delete') {
    return 'Deleting'
  }
  if (operation.kind === 'compress') {
    return 'Compressing'
  }
  if (operation.kind === 'extract') {
    return 'Extracting'
  }
  return operation.kind === 'move' ? 'Moving' : 'Copying'
}

function itemSummary(operation: OpProgress) {
  if (operation.status === 'pending' && operation.itemNames.length > 0) {
    if (operation.itemNames.length <= 2) {
      return operation.itemNames.join(', ')
    }
    return `${operation.itemNames.slice(0, 2).join(', ')}, +${formatCount(
      operation.itemNames.length - 2,
    )} more`
  }
  if (operation.currentFileName) {
    return operation.currentFileName
  }
  return operation.destinationDir ? operation.destinationDir : operation.sourceDir
}

function title(operation: OpProgress) {
  if (operation.status === 'completed') {
    return `${verb(operation)} complete`
  }
  if (operation.status === 'failed') {
    return `${verb(operation)} failed`
  }
  if (operation.status === 'cancelled') {
    return `${verb(operation)} cancelled`
  }
  return `${verb(operation)} ${formatCount(operation.totalItems)} items`
}

type CompactJobRowProps = {
  operation: OpProgress
  hasConflict: boolean
  reorderable: boolean
  onPause: () => void
  onResume: () => void
  onCancel: () => void
  onDismiss: () => void
  onRetry: () => void
  onResolve: () => void
  onMoveUp?: () => void
  onMoveDown?: () => void
}

function CompactJobRow({
  operation,
  hasConflict,
  reorderable,
  onPause,
  onResume,
  onCancel,
  onDismiss,
  onRetry,
  onResolve,
  onMoveUp,
  onMoveDown,
}: CompactJobRowProps) {
  const percent = Math.round(Math.min(100, Math.max(0, operation.progressPercent)))
  const failed = operation.status === 'failed'
  const completed = operation.status === 'completed'
  const cancelled = operation.status === 'cancelled'
  const paused = operation.status === 'paused'
  const pending = operation.status === 'pending'
  const conflict = hasConflict || operation.status === 'conflict'

  return (
    <div className="flex items-center gap-2 border-t border-light-border px-4 py-2.5 dark:border-dark-border">
      {reorderable ? (
        <div className="flex shrink-0 items-center">
          <button
            type="button"
            aria-label="Move job up"
            onClick={onMoveUp}
            disabled={!onMoveUp}
            className="flex h-6 w-6 items-center justify-center rounded-md text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover disabled:opacity-40 dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <ChevronUpIcon className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            aria-label="Move job down"
            onClick={onMoveDown}
            disabled={!onMoveDown}
            className="flex h-6 w-6 items-center justify-center rounded-md text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover disabled:opacity-40 dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <ChevronDownIcon className="h-3.5 w-3.5" />
          </button>
        </div>
      ) : (
        <GripVerticalIcon className="h-4 w-4 shrink-0 text-light-text-faint dark:text-dark-text-faint" />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          {completed ? <CheckCircleIcon className="h-3.5 w-3.5 shrink-0 text-accent-green" /> : null}
          {failed ? <XCircleIcon className="h-3.5 w-3.5 shrink-0 text-accent-red" /> : null}
          {cancelled ? (
            <XCircleIcon className="h-3.5 w-3.5 shrink-0 text-light-text-muted dark:text-dark-text-muted" />
          ) : null}
          <span className="truncate text-xs font-semibold text-light-text dark:text-dark-text">
            {title(operation)}
          </span>
        </div>
        <div className="mt-1 truncate font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
          {itemSummary(operation)}
        </div>
      </div>
      <span
        className={`w-10 shrink-0 text-right font-mono text-xs font-semibold ${
          failed ? 'text-accent-red' : 'text-accent-blue-light dark:text-accent-blue'
        }`}
      >
        {percent}%
      </span>
      <div className="flex shrink-0 items-center gap-1">
        {conflict ? (
          <button
            type="button"
            aria-label="Resolve conflict"
            onClick={onResolve}
            className="flex h-7 w-7 items-center justify-center rounded-md bg-accent-blue-soft text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <PlayIcon className="h-3.5 w-3.5" />
          </button>
        ) : failed ? (
          <button
            type="button"
            aria-label="Retry job"
            onClick={onRetry}
            className="flex h-7 w-7 items-center justify-center rounded-md bg-accent-blue-soft text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <RotateCcwIcon className="h-3.5 w-3.5" />
          </button>
        ) : completed || cancelled ? (
          <button
            type="button"
            aria-label="Dismiss job"
            onClick={onDismiss}
            className="flex h-7 w-7 items-center justify-center rounded-md text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <XIcon className="h-3.5 w-3.5" />
          </button>
        ) : pending ? null : paused ? (
          <button
            type="button"
            aria-label="Resume job"
            onClick={onResume}
            className="flex h-7 w-7 items-center justify-center rounded-md bg-accent-blue-soft text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <PlayIcon className="h-3.5 w-3.5" />
          </button>
        ) : (
          <button
            type="button"
            aria-label="Pause job"
            onClick={onPause}
            className="flex h-7 w-7 items-center justify-center rounded-md bg-accent-blue-soft text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <PauseIcon className="h-3.5 w-3.5" />
          </button>
        )}
        {!completed && !failed && !cancelled ? (
          <button
            type="button"
            aria-label="Cancel job"
            onClick={onCancel}
            className="flex h-7 w-7 items-center justify-center rounded-md text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <XIcon className="h-3.5 w-3.5" />
          </button>
        ) : null}
      </div>
    </div>
  )
}

export function QueuePanel() {
  const order = useQueueStore((state) => state.order)
  const operationsMap = useQueueStore((state) => state.operations)
  const throughputHistory = useQueueStore((state) => state.throughputHistory)
  const throughputPeak = useQueueStore((state) => state.throughputPeak)
  const operations = useMemo(
    () =>
      order
        .map((id) => operationsMap[id])
        .filter((operation): operation is OpProgress => operation !== undefined),
    [order, operationsMap],
  )
  const conflicts = useQueueStore((state) => state.conflicts)
  const collapse = useQueueStore((state) => state.setExpanded)
  const pause = useQueueStore((state) => state.pause)
  const resume = useQueueStore((state) => state.resume)
  const cancel = useQueueStore((state) => state.cancel)
  const retry = useQueueStore((state) => state.retry)
  const dismiss = useQueueStore((state) => state.dismissOperation)
  const setExpanded = useQueueStore((state) => state.setExpanded)
  const primary = primaryOperation(operations, conflicts)
  const secondary = primary
    ? operations.filter((operation) => operation.operationId !== primary.operationId)
    : []

  const activeCount = operations.filter((operation) => !isTerminal(operation.status)).length
  const activeLabel = activeCount === 1 ? '1 active job' : `${activeCount} active jobs`

  return (
    <section
      role="region"
      aria-label="Job queue"
      className="flex h-queue-list w-copycard flex-col overflow-hidden rounded-window border border-light-border-strong bg-light-surface shadow-float dark:border-dark-border-strong dark:bg-dark-surface"
    >
      <header className="flex items-center justify-between gap-2 border-b border-light-border px-4 py-2.5 dark:border-dark-border">
        <span className="text-xs font-semibold text-light-text dark:text-dark-text">
          {activeCount > 0 ? activeLabel : 'Jobs'}
        </span>
        <button
          type="button"
          onClick={() => collapse(false)}
          className="flex items-center gap-1 rounded-md px-2 py-1 text-uxs text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
        >
          Fewer details
          <ChevronDownIcon className="h-3.5 w-3.5" />
        </button>
      </header>
      <div className="flex min-h-0 flex-1 flex-col">
        {primary ? (
          <JobCard
            operation={primary}
            throughputHistory={(throughputHistory[primary.operationId] ?? []) as ThroughputSample[]}
            throughputPeak={throughputPeak[primary.operationId] ?? 0}
            hasConflict={Boolean(conflicts[primary.operationId])}
            reorderable={primary.status === 'pending'}
            onPause={() => pause(primary.operationId)}
            onResume={() => resume(primary.operationId)}
            onCancel={() => cancel(primary.operationId)}
            onDismiss={() => dismiss(primary.operationId)}
            onSkip={() => cancel(primary.operationId)}
            onRetry={() => retry(primary.operationId)}
            onResolve={() => setExpanded(true)}
            onMoveUp={
              primary.status === 'pending'
                ? () => reorder(operations, primary.operationId, -1)
                : undefined
            }
            onMoveDown={
              primary.status === 'pending'
                ? () => reorder(operations, primary.operationId, 1)
                : undefined
            }
          />
        ) : null}
        {secondary.length > 0 ? (
          <div
            aria-label="Queued jobs"
            className="min-h-0 flex-1 overflow-auto scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint"
          >
            {secondary.map((operation) => {
              const reorderable = operation.status === 'pending'
              return (
                <CompactJobRow
                  key={operation.operationId}
                  operation={operation}
                  hasConflict={Boolean(conflicts[operation.operationId])}
                  reorderable={reorderable}
                  onPause={() => pause(operation.operationId)}
                  onResume={() => resume(operation.operationId)}
                  onCancel={() => cancel(operation.operationId)}
                  onDismiss={() => dismiss(operation.operationId)}
                  onRetry={() => retry(operation.operationId)}
                  onResolve={() => setExpanded(true)}
                  onMoveUp={
                    reorderable ? () => reorder(operations, operation.operationId, -1) : undefined
                  }
                  onMoveDown={
                    reorderable ? () => reorder(operations, operation.operationId, 1) : undefined
                  }
                />
              )
            })}
          </div>
        ) : null}
      </div>
    </section>
  )
}
