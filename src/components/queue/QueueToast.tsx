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
import { DETAIL_SEPARATOR, formatCount, formatDuration } from '@/lib/format'
import type { OpKind, OpProgress } from '@/lib/types/ipc'
import { isTerminal, useQueueStore } from '@/stores/queue-store'
import { FOCUS_RING } from '@/lib/focus-ring'
import { fractionToPercent } from '@/lib/numbers'
import { operationVerbs } from '@/lib/queue-format'
import { TEXT_SECONDARY } from '@/lib/class-names'

const kindIcons: Record<OpKind, typeof CopyIcon> = {
  copy: CopyIcon,
  move: CornerUpRightIcon,
  delete: Trash2Icon,
  compress: PackageIcon,
  extract: PackageOpenIcon,
}

function countLabel(count: number, noun: string) {
  return `${formatCount(count)} ${noun}${count === 1 ? '' : 's'}`
}

function finishedSummary(active: OpProgress[], operations: OpProgress[]) {
  const failed = operations.filter((operation) => operation.status === 'failed')
  const cancelled = operations.filter((operation) => operation.status === 'cancelled')
  if (failed.length > 0) {
    return {
      icon: 'failed' as const,
      label: `${countLabel(failed.length, 'job')} failed`,
      percent: 100,
      detail: null,
      dismissible: active.length === 0,
    }
  }
  if (active.length > 0) {
    return null
  }
  if (cancelled.length > 0) {
    return {
      icon: 'cancelled' as const,
      label: `${countLabel(cancelled.length, 'job')} cancelled`,
      percent: 100,
      detail: null,
      dismissible: true,
    }
  }
  return {
    icon: 'done' as const,
    label: 'Jobs complete',
    percent: 100,
    detail: null,
    dismissible: true,
  }
}

/** One kind names the whole queue, by precedence; the icon and verb follow it. */
function dominantKind(active: OpProgress[]): OpKind {
  return (
    (['delete', 'extract', 'compress', 'move'] as const).find((candidate) =>
      active.some((operation) => operation.kind === candidate),
    ) ?? 'copy'
  )
}

function longestEta(active: OpProgress[]) {
  const etas = active.flatMap((operation) =>
    operation.etaSeconds === null ? [] : [operation.etaSeconds],
  )
  return etas.length > 0 ? [formatDuration(Math.max(...etas))] : []
}

function activeSummary(active: OpProgress[]) {
  const totalBytes = active.reduce((sum, operation) => sum + operation.totalBytes, 0)
  const copiedBytes = active.reduce((sum, operation) => sum + operation.copiedBytes, 0)
  const percent = totalBytes > 0 ? fractionToPercent(copiedBytes / totalBytes) : 0
  const kind = dominantKind(active)
  const verb = operationVerbs[kind]

  return {
    icon: 'active' as const,
    kind,
    label:
      active.length === 1
        ? `${verb} ${countLabel(active[0].totalItems, 'item')}`
        : `${verb} ${formatCount(active.length)} jobs`,
    // Percent and time left answer "how much is left" without expanding.
    detail: [`${Math.round(percent)}%`, ...longestEta(active)].join(DETAIL_SEPARATOR),
    percent,
    dismissible: false,
  }
}

function summarize(operations: OpProgress[]) {
  const active = operations.filter((operation) => !isTerminal(operation.status))
  return finishedSummary(active, operations) ?? activeSummary(active)
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
          <XCircleIcon className={`size-4 shrink-0 ${TEXT_SECONDARY}`} />
        )}
        <button
          type="button"
          onClick={() => expand(true)}
          className={`min-w-0 flex-1 cursor-pointer truncate rounded-control text-left ${FOCUS_RING}`}
        >
          {summary.label}
        </button>
        {summary.detail ? (
          <span className={`shrink-0 text-caption tabular-nums ${TEXT_SECONDARY}`}>
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
