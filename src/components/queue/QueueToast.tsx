import { useMemo } from 'react'
import {
  CheckCircleIcon,
  ChevronUpIcon,
  LoaderCircleIcon,
  XIcon,
  XCircleIcon,
} from '@/components/icons'
import { formatCount } from '@/lib/format'
import type { OpProgress } from '@/lib/types/ipc'
import { isTerminal, useQueueStore } from '@/stores/queue-store'

function summarize(operations: OpProgress[]) {
  const active = operations.filter((operation) => !isTerminal(operation.status))
  const failed = operations.filter((operation) => operation.status === 'failed')
  const cancelled = operations.filter((operation) => operation.status === 'cancelled')
  const allDone = active.length === 0 && failed.length === 0

  if (failed.length > 0) {
    return {
      icon: 'failed' as const,
      label: `${formatCount(failed.length)} job${failed.length === 1 ? '' : 's'} failed`,
      percent: 100,
      dismissible: active.length === 0,
    }
  }
  if (cancelled.length > 0 && active.length === 0) {
    return {
      icon: 'cancelled' as const,
      label: `${formatCount(cancelled.length)} job${cancelled.length === 1 ? '' : 's'} cancelled`,
      percent: 100,
      dismissible: true,
    }
  }
  if (allDone) {
    return {
      icon: 'done' as const,
      label: 'Jobs complete',
      percent: 100,
      dismissible: true,
    }
  }

  const totalBytes = active.reduce((sum, operation) => sum + operation.totalBytes, 0)
  const copiedBytes = active.reduce((sum, operation) => sum + operation.copiedBytes, 0)
  const percent = totalBytes > 0 ? (copiedBytes / totalBytes) * 100 : 0
  const verb = active.some((operation) => operation.kind === 'delete')
    ? 'Deleting'
    : active.some((operation) => operation.kind === 'extract')
      ? 'Extracting'
      : active.some((operation) => operation.kind === 'compress')
        ? 'Compressing'
        : active.some((operation) => operation.kind === 'move')
          ? 'Moving'
          : 'Copying'

  return {
    icon: 'active' as const,
    label: `${verb} ${formatCount(active.length)} job${active.length === 1 ? '' : 's'}`,
    percent,
    dismissible: false,
  }
}

export function QueueToast() {
  const order = useQueueStore((state) => state.order)
  const operationsMap = useQueueStore((state) => state.operations)
  const operations = useMemo(
    () =>
      order
        .map((id) => operationsMap[id])
        .filter((operation): operation is OpProgress => operation !== undefined),
    [order, operationsMap],
  )
  const expand = useQueueStore((state) => state.setExpanded)
  const dismissOperation = useQueueStore((state) => state.dismissOperation)

  const summary = summarize(operations)
  const dismissTerminal = () => {
    for (const operation of operations) {
      if (isTerminal(operation.status)) {
        dismissOperation(operation.operationId)
      }
    }
  }

  return (
    <div className="flex w-copycard items-center gap-2 rounded-window border border-light-border-strong bg-light-surface px-3 py-2.5 shadow-float dark:border-dark-border-strong dark:bg-dark-surface">
      <button
        type="button"
        aria-label="Expand job queue"
        onClick={() => expand(true)}
        className="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
      >
        <span className="flex h-7 w-7 shrink-0 items-center justify-center">
          {summary.icon === 'done' ? (
            <CheckCircleIcon className="h-5 w-5 text-accent-green" />
          ) : summary.icon === 'failed' ? (
            <XCircleIcon className="h-5 w-5 text-accent-red" />
          ) : summary.icon === 'cancelled' ? (
            <XCircleIcon className="h-5 w-5 text-light-text-muted dark:text-dark-text-muted" />
          ) : (
            <LoaderCircleIcon className="h-5 w-5 animate-spin text-accent-blue-light dark:text-accent-blue" />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-xs font-semibold text-light-text dark:text-dark-text">
            {summary.label}
          </span>
          <span className="mt-1.5 block h-1.5 overflow-hidden rounded-full bg-light-skeleton dark:bg-dark-skeleton">
            <span
              className={`block h-full rounded-full ${
                summary.icon === 'failed'
                  ? 'bg-accent-red'
                  : summary.icon === 'done'
                    ? 'bg-accent-green'
                    : summary.icon === 'cancelled'
                      ? 'bg-light-text-muted dark:bg-dark-text-muted'
                      : 'bg-accent-blue-light dark:bg-accent-blue'
              }`}
              style={{ width: `${Math.min(100, Math.max(0, summary.percent))}%` }}
            />
          </span>
        </span>
      </button>
      <div className="flex shrink-0 items-center gap-1">
        <ChevronUpIcon className="h-4 w-4 text-light-text-muted dark:text-dark-text-muted" />
        {summary.dismissible ? (
          <button
            type="button"
            aria-label="Dismiss job queue"
            onClick={dismissTerminal}
            className="flex h-7 w-7 items-center justify-center rounded-md text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <XIcon className="h-4 w-4" />
          </button>
        ) : null}
      </div>
    </div>
  )
}
