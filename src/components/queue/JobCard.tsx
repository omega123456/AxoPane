import { useId, useState, type ReactNode } from 'react'
import { CheckIcon, ChevronDownIcon, ChevronUpIcon } from '@/components/icons'
import { Button, ProgressBar } from '@/components/controls'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import { DETAIL_SEPARATOR, formatCount, formatRate } from '@/lib/format'
import { formatItemPreview, formatJobTitle, jobFlags, type JobFlags } from '@/lib/queue-format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'
import { clampPercent } from '@/lib/numbers'
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { JobStatusIcon } from '@/components/queue/JobStatusIcon'
import { useJobCardView, type LiveJobView } from '@/components/queue/use-job-card-view'
import { JobActions, MoveUpHandle } from '@/components/queue/JobActions'

type JobCardProps = {
  operation: OpProgress
  throughputHistory: ThroughputSample[]
  throughputPeak: number
  hasConflict: boolean
  reorderable: boolean
  onPause: () => void
  onResume: () => void
  onCancel: () => void
  onDismiss: () => void
  onSkip: () => void
  onRetry: () => void
  onResolve: () => void
  onMoveUp?: () => void
  onMoveDown?: () => void
}

/** Where the items go; a delete has no destination, so it names what it removes. */
function jobDestination(operation: OpProgress) {
  const preview =
    formatItemPreview(operation.itemNames, operation.totalItems) ?? operation.sourceDir
  if (operation.status === 'pending') {
    return `Waiting${DETAIL_SEPARATOR}${preview}`
  }
  return operation.destinationDir ? `to ${operation.destinationDir}` : preview
}

function ItemsToggle({
  expanded,
  listId,
  onToggle,
}: {
  expanded: boolean
  listId: string
  onToggle: () => void
}) {
  const label = expanded ? 'Hide items' : 'Show items'
  return (
    <Button
      variant="subtle"
      aria-expanded={expanded}
      aria-controls={listId}
      aria-label={label}
      title={label}
      onClick={onToggle}
      size="compact"
    >
      {expanded ? <ChevronUpIcon className="size-3" /> : <ChevronDownIcon className="size-3" />}
    </Button>
  )
}

function itemNameClass(done: boolean, current: boolean) {
  if (done) {
    return 'text-light-text-tertiary dark:text-dark-text-tertiary'
  }
  return current ? 'text-light-text dark:text-dark-text' : ''
}

function JobItemList({
  listId,
  names,
  completedItems,
  isFinished,
  unlistedCount,
}: {
  listId: string
  names: string[]
  completedItems: number
  isFinished: boolean
  unlistedCount: number
}) {
  return (
    <ul
      id={listId}
      className={`flex max-h-queue-items flex-col gap-0.5 overflow-y-auto rounded-control bg-light-recessed px-2.5 py-1.5 text-caption ${TEXT_SECONDARY} ${SCROLLBAR_THIN} dark:bg-dark-recessed`}
    >
      {names.map((name, index) => {
        const done = index < completedItems
        const current = !isFinished && index === completedItems
        return (
          <li
            key={name}
            className={`flex items-center gap-1 truncate ${itemNameClass(done, current)}`}
          >
            {done ? (
              <CheckIcon className="size-3 shrink-0 text-light-success dark:text-dark-success" />
            ) : null}
            <span className="truncate">{name}</span>
          </li>
        )
      })}
      {unlistedCount > 0 ? (
        <li className="text-light-text-tertiary dark:text-dark-text-tertiary">
          +{formatCount(unlistedCount)} more
        </li>
      ) : null}
    </ul>
  )
}

function JobProgress({
  percent,
  headingId,
  summary,
  flags,
  rate,
}: {
  percent: number
  headingId: string
  summary: string
  flags: JobFlags
  rate: number
}) {
  return (
    <>
      <ProgressBar
        value={percent}
        labelledBy={headingId}
        tone={flags.isPaused ? 'neutral' : 'accent'}
      />
      <div
        className={`flex min-w-0 justify-between gap-3 text-caption tabular-nums ${TEXT_SECONDARY}`}
      >
        <span className="min-w-0 truncate">{summary}</span>
        {flags.showsTransferRate ? (
          <span className="shrink-0">{flags.isPaused ? '—' : formatRate(rate)}</span>
        ) : null}
      </div>
    </>
  )
}

function JobOutcome({
  operation,
  flags,
  view,
}: {
  operation: OpProgress
  flags: JobFlags
  view: LiveJobView
}) {
  const failureMessage = flags.isFailed ? operation.errorMessage : null
  if (failureMessage) {
    return (
      <div className="rounded-control bg-light-danger-bg px-3 py-2 text-caption text-light-danger dark:bg-dark-danger-bg dark:text-dark-danger">
        {failureMessage}
      </div>
    )
  }
  if (flags.isCancelled) {
    return (
      <div className={`text-caption ${TEXT_SECONDARY}`}>
        Job cancelled. Any completed file changes were kept.
      </div>
    )
  }
  if (!flags.showsTransferRate) {
    return null
  }
  return (
    <ThroughputChart
      samples={view.samples}
      currentPercent={clampPercent(view.operation.progressPercent)}
      peakRate={view.peak}
    />
  )
}

function CurrentFileLine({ fileName, itemIndex }: { fileName: string | null; itemIndex: string }) {
  return (
    <div className="flex min-w-0 justify-between gap-3 text-caption tabular-nums text-light-text-tertiary dark:text-dark-text-tertiary">
      <span className="min-w-0 truncate" title={fileName ?? undefined}>
        {fileName ?? ' '}
      </span>
      <span className="shrink-0">{itemIndex}</span>
    </div>
  )
}

function JobHeading({
  operation,
  headingId,
  moveUpHandle,
  itemsToggle,
}: {
  operation: OpProgress
  headingId: string
  moveUpHandle: ReactNode
  itemsToggle: ReactNode
}) {
  return (
    <div className="flex min-w-0 items-center gap-1.5">
      {moveUpHandle}
      <JobStatusIcon status={operation.status} />
      <span
        id={headingId}
        className="min-w-0 truncate text-body font-semibold text-light-text dark:text-dark-text"
      >
        {formatJobTitle(operation)}
      </span>
      {itemsToggle}
    </div>
  )
}

function JobDestinationLine({
  where,
  percent,
  isUnderway,
}: {
  where: string
  percent: number
  isUnderway: boolean
}) {
  return (
    <div className={`flex min-w-0 justify-between gap-3 text-caption ${TEXT_SECONDARY}`}>
      <span className="min-w-0 truncate" title={where}>
        {where}
      </span>
      {isUnderway ? <span className="shrink-0 tabular-nums">{Math.round(percent)}%</span> : null}
    </div>
  )
}

type JobOverviewProps = {
  operation: OpProgress
  headingId: string
  moveUpHandle: ReactNode
  percent: number
  flags: JobFlags
  completedItems: number
}

function JobOverview({
  operation,
  headingId,
  moveUpHandle,
  percent,
  flags,
  completedItems,
}: JobOverviewProps) {
  const itemListId = useId()
  const [itemsExpanded, setItemsExpanded] = useState(false)
  const listedNames = operation.itemNames
  return (
    <>
      <JobHeading
        operation={operation}
        headingId={headingId}
        moveUpHandle={moveUpHandle}
        itemsToggle={
          listedNames.length > 0 ? (
            <ItemsToggle
              expanded={itemsExpanded}
              listId={itemListId}
              onToggle={() => setItemsExpanded((expanded) => !expanded)}
            />
          ) : null
        }
      />
      <JobDestinationLine
        where={jobDestination(operation)}
        percent={percent}
        isUnderway={flags.isUnderway}
      />
      {itemsExpanded ? (
        <JobItemList
          listId={itemListId}
          names={listedNames}
          // The backend sends a bounded preview, so a very large selection can still
          // have names beyond the ones it shipped. Say how many are unlisted rather
          // than implying the list is complete.
          unlistedCount={Math.max(0, operation.totalItems - listedNames.length)}
          completedItems={completedItems}
          isFinished={flags.isFinished}
        />
      ) : null}
    </>
  )
}

function JobLiveStatus({
  operation,
  flags,
  percent,
  headingId,
  jobView,
}: {
  operation: OpProgress
  flags: JobFlags
  percent: number
  headingId: string
  jobView: ReturnType<typeof useJobCardView>
}) {
  const { view, progressSegments, itemIndex, liveAnnouncement, lastCurrentFile } = jobView
  return (
    <>
      <span className="sr-only" aria-live="polite" aria-atomic="true">
        {liveAnnouncement}
      </span>
      {flags.isUnderway ? (
        <JobProgress
          percent={percent}
          headingId={headingId}
          summary={progressSegments.join(DETAIL_SEPARATOR)}
          flags={flags}
          rate={view.rate}
        />
      ) : null}
      <JobOutcome operation={operation} flags={flags} view={view} />
      {flags.isUnderway ? (
        <CurrentFileLine fileName={lastCurrentFile} itemIndex={itemIndex} />
      ) : null}
    </>
  )
}

export function JobCard({
  operation,
  throughputHistory,
  throughputPeak,
  hasConflict,
  reorderable,
  onMoveUp,
  onMoveDown,
  ...handlers
}: JobCardProps) {
  const headingId = useId()
  const percent = clampPercent(operation.progressPercent)
  const flags = jobFlags(operation, hasConflict)

  const jobView = useJobCardView(operation, throughputHistory, throughputPeak, flags)

  return (
    <article
      aria-labelledby={headingId}
      data-status={operation.status}
      className="grid gap-2 px-4 py-3.5"
    >
      <JobOverview
        operation={operation}
        headingId={headingId}
        moveUpHandle={reorderable ? <MoveUpHandle onMoveUp={onMoveUp} /> : null}
        percent={percent}
        flags={flags}
        completedItems={jobView.view.operation.completedItems}
      />
      <JobLiveStatus
        operation={operation}
        flags={flags}
        percent={percent}
        headingId={headingId}
        jobView={jobView}
      />
      <JobActions
        flags={flags}
        handlers={handlers}
        onMoveDown={reorderable ? onMoveDown : undefined}
      />
    </article>
  )
}
