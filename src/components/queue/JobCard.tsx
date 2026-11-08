import { useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  CheckIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  GripVerticalIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  SkipForwardIcon,
} from '@/components/icons'
import { Button, ProgressBar } from '@/components/controls'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import { DETAIL_SEPARATOR, formatCount, formatRate } from '@/lib/format'
import {
  formatItemPreview,
  formatJobProgress,
  formatJobTitle,
  jobFlags,
  type JobFlags,
} from '@/lib/queue-format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'
import { clampPercent } from '@/lib/numbers'
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { JobStatusIcon } from '@/components/queue/JobStatusIcon'
import {
  announcementToPublish,
  isDiscoveringBytes,
  retainedCurrentFile,
} from '@/components/queue/job-view-state'

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

const LIVE_REGION_THROTTLE_MS = 5000
/**
 * Cadence the visible metrics refresh at. The backend reports a jittery
 * instantaneous rate many times a second; sampling it on a calm interval keeps
 * the numbers readable (and roughly in step with the averaged chart) instead of
 * flickering on every progress event. The same tick decides whether the backend
 * is still discovering bytes, by comparing successive totals.
 */
const METRICS_REFRESH_MS = 500

/** Where the items go; a delete has no destination, so it names what it removes. */
function jobDestination(operation: OpProgress) {
  const preview =
    formatItemPreview(operation.itemNames, operation.totalItems) ?? operation.sourceDir
  if (operation.status === 'pending') {
    return `Waiting${DETAIL_SEPARATOR}${preview}`
  }
  return operation.destinationDir ? `to ${operation.destinationDir}` : preview
}

/**
 * While a job is working through its queue, "N completed" reads as "item 0 of
 * 2" for the whole time the first item copies, so show the item being worked on
 * (like Windows' "item 1 of 2"). A stopped job shows the true completed count.
 */
function displayedItemNumber(operation: OpProgress, isFinished: boolean) {
  if (isFinished) {
    return operation.completedItems
  }
  return Math.min(operation.completedItems + 1, operation.totalItems)
}

function useLastCurrentFile(currentFileName: string | null) {
  const [lastCurrentFile, setLastCurrentFile] = useState(currentFileName)
  const retainedFile = retainedCurrentFile(lastCurrentFile, currentFileName)
  if (retainedFile !== lastCurrentFile) {
    setLastCurrentFile(retainedFile)
  }
  return retainedFile
}

type LiveJobView = {
  operation: OpProgress
  rate: number
  samples: ThroughputSample[]
  peak: number
}

function useThrottledJobView(liveView: LiveJobView) {
  const [view, setView] = useState({ ...liveView, scanning: false })
  const latestViewRef = useRef(liveView)
  const lastTotalBytesRef = useRef(liveView.operation.totalBytes)
  useEffect(() => {
    latestViewRef.current = liveView
  })
  useEffect(() => {
    const intervalId = window.setInterval(() => {
      const latest = latestViewRef.current
      const scanning = isDiscoveringBytes(latest.operation.totalBytes, lastTotalBytesRef.current)
      lastTotalBytesRef.current = latest.operation.totalBytes
      setView({ ...latest, scanning })
    }, METRICS_REFRESH_MS)
    return () => window.clearInterval(intervalId)
  }, [])
  return view
}

function useThrottledAnnouncement(announcement: string) {
  const [liveAnnouncement, setLiveAnnouncement] = useState(announcement)
  const liveAnnouncementRef = useRef(liveAnnouncement)
  const pendingLiveAnnouncementRef = useRef<string | null>(null)
  const liveRegionTimerRef = useRef<number | null>(null)

  useEffect(() => {
    liveAnnouncementRef.current = liveAnnouncement
  }, [liveAnnouncement])

  useEffect(() => {
    if (announcement === liveAnnouncement) {
      pendingLiveAnnouncementRef.current = null
      if (liveRegionTimerRef.current !== null) {
        window.clearTimeout(liveRegionTimerRef.current)
        liveRegionTimerRef.current = null
      }
      return
    }

    pendingLiveAnnouncementRef.current = announcement
    if (liveRegionTimerRef.current !== null) {
      return
    }

    liveRegionTimerRef.current = window.setTimeout(() => {
      liveRegionTimerRef.current = null
      const nextAnnouncement = announcementToPublish(
        pendingLiveAnnouncementRef.current,
        liveAnnouncementRef.current,
      )
      pendingLiveAnnouncementRef.current = null
      if (nextAnnouncement !== null) {
        setLiveAnnouncement(nextAnnouncement)
        liveAnnouncementRef.current = nextAnnouncement
      }
    }, LIVE_REGION_THROTTLE_MS)
  }, [announcement, liveAnnouncement])

  useEffect(() => {
    return () => {
      if (liveRegionTimerRef.current !== null) {
        window.clearTimeout(liveRegionTimerRef.current)
      }
    }
  }, [])

  return liveAnnouncement
}

/** The chart's smoothed leading-edge rate, so the number matches the curve; the raw rate before any history. */
function currentRate(throughputHistory: ThroughputSample[], operation: OpProgress) {
  return throughputHistory.at(-1)?.rate ?? operation.bytesPerSecond
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

type JobActionHandlers = Pick<
  JobCardProps,
  'onPause' | 'onResume' | 'onCancel' | 'onDismiss' | 'onSkip' | 'onRetry' | 'onResolve'
>

function PrimaryJobActions({ flags, handlers }: { flags: JobFlags; handlers: JobActionHandlers }) {
  if (flags.isConflict) {
    return (
      <Button variant="primary" onClick={handlers.onResolve}>
        Resolve conflict
      </Button>
    )
  }
  if (flags.isFailed) {
    return (
      <>
        <Button onClick={handlers.onRetry}>
          <RotateCcwIcon className="size-4" /> Retry
        </Button>
        <Button variant="subtle" onClick={handlers.onDismiss}>
          Dismiss
        </Button>
      </>
    )
  }
  if (flags.isFinished) {
    return <Button onClick={handlers.onDismiss}>Dismiss</Button>
  }
  if (flags.isPaused) {
    return (
      <Button onClick={handlers.onResume}>
        <PlayIcon className="size-4" /> Resume
      </Button>
    )
  }
  if (flags.isPending) {
    return null
  }
  return (
    <Button onClick={handlers.onPause}>
      <PauseIcon className="size-4" /> Pause
    </Button>
  )
}

function JobActions({
  flags,
  handlers,
  onMoveDown,
}: {
  flags: JobFlags
  handlers: JobActionHandlers
  onMoveDown: (() => void) | undefined
}) {
  return (
    <div className="mt-1 flex flex-wrap items-center gap-2">
      <PrimaryJobActions flags={flags} handlers={handlers} />

      {flags.isUnderway ? (
        <Button onClick={handlers.onSkip} disabled={!flags.isPaused}>
          <SkipForwardIcon className="size-4" /> Skip
        </Button>
      ) : null}

      {!flags.isFinished ? (
        <Button variant="subtle" onClick={handlers.onCancel}>
          Cancel
        </Button>
      ) : null}

      {onMoveDown ? (
        <Button
          variant="subtle"
          aria-label="Move job down"
          onClick={onMoveDown}
          className="ml-auto"
        >
          Move down
        </Button>
      ) : null}
    </div>
  )
}

function MoveUpHandle({ onMoveUp }: { onMoveUp: (() => void) | undefined }) {
  return (
    <Button
      variant="subtle"
      aria-label="Move job up"
      onClick={onMoveUp}
      disabled={!onMoveUp}
      size="compact"
      className="-ml-2"
    >
      <GripVerticalIcon className="size-3.5" />
    </Button>
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
  const itemListId = useId()
  const percent = clampPercent(operation.progressPercent)
  const flags = jobFlags(operation, hasConflict)

  const listedNames = operation.itemNames
  // The backend sends a bounded preview, so a very large selection can still
  // have names beyond the ones it shipped. Say how many are unlisted rather
  // than implying the list is complete.
  const unlistedCount = Math.max(0, operation.totalItems - listedNames.length)
  const [itemsExpanded, setItemsExpanded] = useState(false)

  const lastCurrentFile = useLastCurrentFile(operation.currentFileName)
  const rate = currentRate(throughputHistory, operation)
  const liveView = useMemo(
    () => ({ operation, rate, samples: throughputHistory, peak: throughputPeak }),
    [rate, operation, throughputHistory, throughputPeak],
  )
  const view = useThrottledJobView(liveView)

  const displayed = view.operation
  const progressSegments = useMemo(
    () => formatJobProgress(displayed, { scanning: view.scanning }),
    [displayed, view.scanning],
  )
  const itemIndex = `item ${formatCount(displayedItemNumber(displayed, flags.isFinished))} of ${formatCount(displayed.totalItems)}`
  const liveAnnouncement = useThrottledAnnouncement(progressSegments.join(', '))
  const where = jobDestination(operation)

  return (
    <article
      aria-labelledby={headingId}
      data-status={operation.status}
      className="grid gap-2 px-4 py-3.5"
    >
      <div className="flex min-w-0 items-center gap-1.5">
        {reorderable ? <MoveUpHandle onMoveUp={onMoveUp} /> : null}
        <JobStatusIcon status={operation.status} />
        <span
          id={headingId}
          className="min-w-0 truncate text-body font-semibold text-light-text dark:text-dark-text"
        >
          {formatJobTitle(operation)}
        </span>
        {listedNames.length > 0 ? (
          <ItemsToggle
            expanded={itemsExpanded}
            listId={itemListId}
            onToggle={() => setItemsExpanded((expanded) => !expanded)}
          />
        ) : null}
      </div>

      <div className={`flex min-w-0 justify-between gap-3 text-caption ${TEXT_SECONDARY}`}>
        <span className="min-w-0 truncate" title={where}>
          {where}
        </span>
        {flags.isUnderway ? (
          <span className="shrink-0 tabular-nums">{Math.round(percent)}%</span>
        ) : null}
      </div>

      {itemsExpanded ? (
        <JobItemList
          listId={itemListId}
          names={listedNames}
          completedItems={displayed.completedItems}
          isFinished={flags.isFinished}
          unlistedCount={unlistedCount}
        />
      ) : null}

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

      <JobActions
        flags={flags}
        handlers={handlers}
        onMoveDown={reorderable ? onMoveDown : undefined}
      />
    </article>
  )
}
