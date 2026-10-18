import { useMemo } from 'react'
import { Button } from '@/components/controls'
import {
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  XCircleIcon,
  XIcon,
} from '@/components/icons'
import { JobCard } from '@/components/queue/JobCard'
import { reorderOps } from '@/lib/queue-commands'
import { formatCount } from '@/lib/format'
import { formatItemPreview, verb } from '@/lib/queue-format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'
import { useQueueStore } from '@/stores/queue-store'

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
    const nonPending = state.order.filter((operationId) => !pending.includes(operationId))
    return { order: [...nonPending, ...next] }
  })
}

function primaryOperation(operations: OpProgress[], conflicts: Record<string, unknown>) {
  return (
    operations.find(
      (operation) => conflicts[operation.operationId] || operation.status === 'conflict',
    ) ??
    operations.find(
      (operation) => operation.status === 'active' || operation.status === 'paused',
    ) ??
    operations.find((operation) => operation.status === 'pending') ??
    operations.at(-1)
  )
}

function itemSummary(operation: OpProgress) {
  if (operation.status === 'pending' && operation.itemNames.length > 0) {
    return formatItemPreview(operation.itemNames, operation.totalItems)
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

  const summaryText = pending ? `Waiting · ${itemSummary(operation)}` : itemSummary(operation)

  return (
    <div className="flex items-center gap-3 border-t border-light-divider pb-3 pl-4 pr-2 pt-2.5 dark:border-dark-divider">
      <div className="grid min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          {completed ? (
            <CheckCircleIcon className="size-4 shrink-0 text-light-success dark:text-dark-success" />
          ) : null}
          {failed ? (
            <XCircleIcon className="size-4 shrink-0 text-light-danger dark:text-dark-danger" />
          ) : null}
          {cancelled ? (
            <XCircleIcon className="size-4 shrink-0 text-light-text-secondary dark:text-dark-text-secondary" />
          ) : null}
          <span className="truncate text-body font-semibold text-light-text dark:text-dark-text">
            {title(operation)}
          </span>
        </div>
        <span className="truncate text-caption text-light-text-secondary dark:text-dark-text-secondary">
          {summaryText}
          {!pending && !completed && !cancelled ? ` · ${percent}%` : ''}
        </span>
      </div>
      <div className="flex shrink-0 items-center">
        {reorderable ? (
          <>
            <Button
              variant="subtle"
              aria-label="Move job up"
              onClick={onMoveUp}
              disabled={!onMoveUp}
              size="icon"
            >
              <ChevronUpIcon className="size-4" />
            </Button>
            <Button
              variant="subtle"
              aria-label="Move job down"
              onClick={onMoveDown}
              disabled={!onMoveDown}
              size="icon"
            >
              <ChevronDownIcon className="size-4" />
            </Button>
          </>
        ) : null}
        {conflict ? (
          <Button variant="subtle" aria-label="Resolve conflict" onClick={onResolve} size="icon">
            <PlayIcon className="size-4" />
          </Button>
        ) : failed ? (
          <Button variant="subtle" aria-label="Retry job" onClick={onRetry} size="icon">
            <RotateCcwIcon className="size-4" />
          </Button>
        ) : completed || cancelled ? (
          <Button variant="subtle" aria-label="Dismiss job" onClick={onDismiss} size="icon">
            <XIcon className="size-4" />
          </Button>
        ) : pending ? null : paused ? (
          <Button variant="subtle" aria-label="Resume job" onClick={onResume} size="icon">
            <PlayIcon className="size-4" />
          </Button>
        ) : (
          <Button variant="subtle" aria-label="Pause job" onClick={onPause} size="icon">
            <PauseIcon className="size-4" />
          </Button>
        )}
        {!completed && !failed && !cancelled ? (
          <Button
            variant="subtle"
            aria-label="Cancel job"
            title={pending ? 'Remove from queue' : 'Cancel'}
            onClick={onCancel}
            size="icon"
          >
            <XIcon className="size-4" />
          </Button>
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
  const skip = useQueueStore((state) => state.skip)
  const retry = useQueueStore((state) => state.retry)
  const dismiss = useQueueStore((state) => state.dismissOperation)
  const setExpanded = useQueueStore((state) => state.setExpanded)
  const primary = primaryOperation(operations, conflicts)
  const secondary = primary
    ? operations.filter((operation) => operation.operationId !== primary.operationId)
    : []

  const jobsLabel = operations.length === 1 ? '1 job' : `${formatCount(operations.length)} jobs`

  return (
    <section
      role="region"
      aria-label="Job queue"
      className="flex max-h-queue-list w-queue flex-col overflow-hidden rounded-overlay border border-light-flyout-stroke bg-light-card text-light-text shadow-light-flyout dark:border-dark-flyout-stroke dark:bg-dark-card dark:text-dark-text dark:shadow-dark-flyout"
    >
      <header className="flex h-11 flex-none items-center justify-between gap-2 border-b border-light-divider pl-4 pr-2 dark:border-dark-divider">
        <span className="text-body font-semibold">{jobsLabel}</span>
        <Button
          variant="subtle"
          aria-label="Fewer details"
          title="Collapse"
          onClick={() => collapse(false)}
          size="icon"
        >
          <ChevronDownIcon className="size-4" />
        </Button>
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
            onSkip={() => skip(primary.operationId)}
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
            className="min-h-0 flex-1 overflow-auto scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary"
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
