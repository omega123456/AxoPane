import { Button } from '@/components/controls'
import {
  ChevronDownIcon,
  ChevronUpIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  XIcon,
} from '@/components/icons'
import { JobCard } from '@/components/queue/JobCard'
import { reorderOps } from '@/lib/queue-commands'
import { DETAIL_SEPARATOR, formatCount } from '@/lib/format'
import { formatItemPreview, formatJobTitle, jobFlags, type JobFlags } from '@/lib/queue-format'
import { JobStatusIcon } from '@/components/queue/JobStatusIcon'
import type { OpProgress } from '@/lib/types/ipc'
import { useQueueStore } from '@/stores/queue-store'
import { withReorderedPending } from '@/stores/queue/operations'
import { throughputTrackOf } from '@/stores/queue/throughput'
import { clampPercent } from '@/lib/numbers'
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { useOrderedOperations } from './use-ordered-operations'

function movePendingOperation(operations: OpProgress[], id: string, delta: number) {
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
  useQueueStore.setState((state) => withReorderedPending(state, pending, next))
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

function compactSummary(operation: OpProgress, flags: JobFlags, percent: number) {
  const summary = itemSummary(operation)
  const text = flags.isPending ? `Waiting${DETAIL_SEPARATOR}${summary}` : summary
  const showsPercent = !flags.isPending && !flags.isCompleted && !flags.isCancelled
  return showsPercent ? `${text}${DETAIL_SEPARATOR}${percent}%` : text
}

type CompactActionHandlers = Pick<
  CompactJobRowProps,
  'onPause' | 'onResume' | 'onDismiss' | 'onRetry' | 'onResolve'
>

function CompactPrimaryAction({
  flags,
  handlers,
}: {
  flags: JobFlags
  handlers: CompactActionHandlers
}) {
  if (flags.isConflict) {
    return (
      <Button
        variant="subtle"
        aria-label="Resolve conflict"
        onClick={handlers.onResolve}
        size="icon"
      >
        <PlayIcon className="size-4" />
      </Button>
    )
  }
  if (flags.isFailed) {
    return (
      <Button variant="subtle" aria-label="Retry job" onClick={handlers.onRetry} size="icon">
        <RotateCcwIcon className="size-4" />
      </Button>
    )
  }
  if (flags.isFinished) {
    return (
      <Button variant="subtle" aria-label="Dismiss job" onClick={handlers.onDismiss} size="icon">
        <XIcon className="size-4" />
      </Button>
    )
  }
  if (flags.isPending) {
    return null
  }
  return flags.isPaused ? (
    <Button variant="subtle" aria-label="Resume job" onClick={handlers.onResume} size="icon">
      <PlayIcon className="size-4" />
    </Button>
  ) : (
    <Button variant="subtle" aria-label="Pause job" onClick={handlers.onPause} size="icon">
      <PauseIcon className="size-4" />
    </Button>
  )
}

function ReorderButtons({
  onMoveUp,
  onMoveDown,
}: {
  onMoveUp: (() => void) | undefined
  onMoveDown: (() => void) | undefined
}) {
  return (
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
  )
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
  onCancel,
  onMoveUp,
  onMoveDown,
  ...handlers
}: CompactJobRowProps) {
  const percent = Math.round(clampPercent(operation.progressPercent))
  const flags = jobFlags(operation, hasConflict)

  return (
    <div className="flex items-center gap-3 border-t border-light-divider pb-3 pl-4 pr-2 pt-2.5 dark:border-dark-divider">
      <div className="grid min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <JobStatusIcon status={operation.status} />
          <span className="truncate text-body font-semibold text-light-text dark:text-dark-text">
            {formatJobTitle(operation)}
          </span>
        </div>
        <span className={`truncate text-caption ${TEXT_SECONDARY}`}>
          {compactSummary(operation, flags, percent)}
        </span>
      </div>
      <div className="flex shrink-0 items-center">
        {reorderable ? <ReorderButtons onMoveUp={onMoveUp} onMoveDown={onMoveDown} /> : null}
        <CompactPrimaryAction flags={flags} handlers={handlers} />
        {!flags.isFinished ? (
          <Button
            variant="subtle"
            aria-label="Cancel job"
            title={flags.isPending ? 'Remove from queue' : 'Cancel'}
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

function operationHandlers(operation: OpProgress, operations: OpProgress[]) {
  const queue = useQueueStore.getState()
  const id = operation.operationId
  const reorderable = operation.status === 'pending'
  return {
    onPause: () => queue.pause(id),
    onResume: () => queue.resume(id),
    onCancel: () => queue.cancel(id),
    onDismiss: () => queue.dismissOperation(id),
    onSkip: () => queue.skip(id),
    onRetry: () => queue.retry(id),
    onResolve: () => queue.setExpanded(true),
    onMoveUp: reorderable ? () => movePendingOperation(operations, id, -1) : undefined,
    onMoveDown: reorderable ? () => movePendingOperation(operations, id, 1) : undefined,
  }
}

export function QueuePanel() {
  const throughput = useQueueStore((state) => state.throughput)
  const operations = useOrderedOperations()
  const conflicts = useQueueStore((state) => state.conflicts)
  const collapse = useQueueStore((state) => state.setExpanded)
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
            throughputHistory={throughputTrackOf(throughput, primary.operationId).history}
            throughputPeak={throughputTrackOf(throughput, primary.operationId).peak}
            hasConflict={Boolean(conflicts[primary.operationId])}
            reorderable={primary.status === 'pending'}
            {...operationHandlers(primary, operations)}
          />
        ) : null}
        {secondary.length > 0 ? (
          <div
            aria-label="Queued jobs"
            className={`min-h-0 flex-1 overflow-auto ${SCROLLBAR_THIN}`}
          >
            {secondary.map((operation) => (
              <CompactJobRow
                key={operation.operationId}
                operation={operation}
                hasConflict={Boolean(conflicts[operation.operationId])}
                reorderable={operation.status === 'pending'}
                {...operationHandlers(operation, operations)}
              />
            ))}
          </div>
        ) : null}
      </div>
    </section>
  )
}
