import { useMemo } from 'react'
import { ChevronDownIcon } from '@/components/icons'
import { JobCard } from '@/components/queue/JobCard'
import { reorderOps } from '@/lib/queue-commands'
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

  const activeCount = operations.filter((operation) => !isTerminal(operation.status)).length

  return (
    <section
      role="region"
      aria-label="Transfer queue"
      className="w-copycard overflow-hidden rounded-window border border-light-border-strong bg-light-surface shadow-float dark:border-dark-border-strong dark:bg-dark-surface"
    >
      <header className="flex items-center justify-between gap-2 border-b border-light-border px-4 py-2.5 dark:border-dark-border">
        <span className="text-xs font-semibold text-light-text dark:text-dark-text">
          {activeCount > 0 ? `${activeCount} active transfers` : 'Transfers'}
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
      <div className="max-h-queue-list overflow-auto">
        {operations.map((operation) => {
          const reorderable = operation.status === 'pending'
          const samples = (throughputHistory[operation.operationId] ?? []) as ThroughputSample[]
          return (
            <JobCard
              key={operation.operationId}
              operation={operation}
              throughputHistory={samples}
              throughputPeak={throughputPeak[operation.operationId] ?? 0}
              hasConflict={Boolean(conflicts[operation.operationId])}
              reorderable={reorderable}
              onPause={() => pause(operation.operationId)}
              onResume={() => resume(operation.operationId)}
              onCancel={() => cancel(operation.operationId)}
              onDismiss={() => dismiss(operation.operationId)}
              onSkip={() => cancel(operation.operationId)}
              onRetry={() => retry(operation.operationId)}
              onResolve={() => setExpanded(true)}
              onMoveUp={reorderable ? () => reorder(operations, operation.operationId, -1) : undefined}
              onMoveDown={
                reorderable ? () => reorder(operations, operation.operationId, 1) : undefined
              }
            />
          )
        })}
      </div>
    </section>
  )
}
