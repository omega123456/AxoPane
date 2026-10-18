import { useMemo } from 'react'
import {
  CheckCircleIcon,
  ChevronUpIcon,
  CopyIcon,
  CornerUpRightIcon,
  PackageIcon,
  PackageOpenIcon,
  Trash2Icon,
  XIcon,
  XCircleIcon,
} from '@/components/icons'
import { Button, ProgressBar } from '@/components/controls'
import { formatCount, formatDuration } from '@/lib/format'
import type { OpKind, OpProgress } from '@/lib/types/ipc'
import { isTerminal, useQueueStore } from '@/stores/queue-store'
import { FOCUS_RING } from '@/lib/focus-ring'

const kindIcons: Record<OpKind, typeof CopyIcon> = {
  copy: CopyIcon,
  move: CornerUpRightIcon,
  delete: Trash2Icon,
  compress: PackageIcon,
  extract: PackageOpenIcon,
}

const kindVerbs: Record<OpKind, string> = {
  copy: 'Copying',
  move: 'Moving',
  delete: 'Deleting',
  compress: 'Compressing',
  extract: 'Extracting',
}

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
      detail: null,
      dismissible: active.length === 0,
    }
  }
  if (cancelled.length > 0 && active.length === 0) {
    return {
      icon: 'cancelled' as const,
      label: `${formatCount(cancelled.length)} job${cancelled.length === 1 ? '' : 's'} cancelled`,
      percent: 100,
      detail: null,
      dismissible: true,
    }
  }
  if (allDone) {
    return {
      icon: 'done' as const,
      label: 'Jobs complete',
      percent: 100,
      detail: null,
      dismissible: true,
    }
  }

  const totalBytes = active.reduce((sum, operation) => sum + operation.totalBytes, 0)
  const copiedBytes = active.reduce((sum, operation) => sum + operation.copiedBytes, 0)
  const percent = totalBytes > 0 ? (copiedBytes / totalBytes) * 100 : 0
  const etas = active.flatMap((operation) =>
    operation.etaSeconds === null ? [] : [operation.etaSeconds],
  )
  // One kind names the whole queue, by precedence; the icon and verb follow it.
  const kind: OpKind =
    (['delete', 'extract', 'compress', 'move'] as const).find((candidate) =>
      active.some((operation) => operation.kind === candidate),
    ) ?? 'copy'
  const verb = kindVerbs[kind]
  const totalItems = active[0].totalItems

  return {
    icon: 'active' as const,
    kind,
    label:
      active.length === 1
        ? `${verb} ${formatCount(totalItems)} item${totalItems === 1 ? '' : 's'}`
        : `${verb} ${formatCount(active.length)} jobs`,
    // Percent and time left answer "how much is left" without expanding.
    detail: [
      `${Math.round(percent)}%`,
      ...(etas.length > 0 ? [formatDuration(Math.max(...etas))] : []),
    ].join(' · '),
    percent,
    dismissible: false,
  }
}

/** The collapsed queue: one line with a progress bar along its bottom edge. */
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
  const KindIcon = summary.icon === 'active' ? kindIcons[summary.kind] : null

  return (
    <div className="grid w-queue-mini gap-2 rounded-overlay border border-light-flyout-stroke bg-light-card pl-3.5 pr-2 pt-2.5 text-light-text shadow-light-flyout dark:border-dark-flyout-stroke dark:bg-dark-card dark:text-dark-text dark:shadow-dark-flyout">
      <div className="flex items-center gap-2.5 text-row">
        {KindIcon ? (
          <KindIcon className="size-4 shrink-0 text-light-accent dark:text-dark-accent" />
        ) : summary.icon === 'done' ? (
          <CheckCircleIcon className="size-4 shrink-0 text-light-success dark:text-dark-success" />
        ) : summary.icon === 'failed' ? (
          <XCircleIcon className="size-4 shrink-0 text-light-danger dark:text-dark-danger" />
        ) : (
          <XCircleIcon className="size-4 shrink-0 text-light-text-secondary dark:text-dark-text-secondary" />
        )}
        <button
          type="button"
          onClick={() => expand(true)}
          className={`min-w-0 flex-1 cursor-pointer truncate rounded-control text-left ${FOCUS_RING}`}
        >
          {summary.label}
        </button>
        {summary.detail ? (
          <span className="shrink-0 text-caption tabular-nums text-light-text-secondary dark:text-dark-text-secondary">
            {summary.detail}
          </span>
        ) : null}
        {summary.dismissible ? (
          <Button
            variant="subtle"
            aria-label="Dismiss job queue"
            onClick={dismissTerminal}
            size="icon"
          >
            <XIcon className="size-4" />
          </Button>
        ) : null}
        <Button
          variant="subtle"
          aria-label="Expand job queue"
          title="Expand"
          onClick={() => expand(true)}
          size="icon"
        >
          <ChevronUpIcon className="size-4" />
        </Button>
      </div>
      <ProgressBar
        value={summary.percent}
        label={summary.label}
        tone={
          summary.icon === 'failed'
            ? 'danger'
            : summary.icon === 'done'
              ? 'success'
              : summary.icon === 'cancelled'
                ? 'neutral'
                : 'accent'
        }
        className="mr-1.5 rounded-b-none"
      />
    </div>
  )
}
