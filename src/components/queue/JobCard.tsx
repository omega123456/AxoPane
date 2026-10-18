import { useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  CheckIcon,
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  GripVerticalIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  SkipForwardIcon,
  XCircleIcon,
} from '@/components/icons'
import { Button, ProgressBar } from '@/components/controls'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import { formatCount, formatRate } from '@/lib/format'
import { formatItemPreview, formatJobProgress, verb } from '@/lib/queue-format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'

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

export function JobCard({
  operation,
  throughputHistory,
  throughputPeak,
  hasConflict,
  reorderable,
  onPause,
  onResume,
  onCancel,
  onDismiss,
  onSkip,
  onRetry,
  onResolve,
  onMoveUp,
  onMoveDown,
}: JobCardProps) {
  const headingId = useId()
  const itemListId = useId()
  const percent = Math.min(100, Math.max(0, operation.progressPercent))
  const roundedPercent = Math.round(percent)
  const isCompleted = operation.status === 'completed'
  const isFailed = operation.status === 'failed'
  const isPaused = operation.status === 'paused'
  const isCancelled = operation.status === 'cancelled'
  const isConflict = operation.status === 'conflict' || hasConflict
  const isPending = operation.status === 'pending'
  const isFinished = isCompleted || isFailed || isCancelled
  const showProgress = !isFinished
  const showCurrentFile = !isPending && !isFinished
  // A delete is measured in items, so a transfer rate and a throughput curve
  // would describe something the rest of the card does not report.
  const isDelete = operation.kind === 'delete'
  const showChart = showCurrentFile && !isDelete
  const showRate = showCurrentFile && !isDelete

  const listedNames = operation.itemNames
  // The backend sends a bounded preview, so a very large selection can still
  // have names beyond the ones it shipped. Say how many are unlisted rather
  // than implying the list is complete.
  const unlistedCount = Math.max(0, operation.totalItems - listedNames.length)
  const [itemsExpanded, setItemsExpanded] = useState(false)

  // The backend briefly clears the current-file fields between finishing one
  // file and starting the next, which would otherwise unmount this whole
  // block and make the card's height (and everything below it) jump. Freeze
  // the last known file in state and keep showing it through that gap; the
  // block itself stays mounted for the whole active-ish lifetime of the job
  // so its reserved space never collapses mid-run. (React's documented
  // "adjusting state during render" pattern — the guarded `setState` below
  // only fires when the snapshot actually changed, so it settles in the same
  // render pass instead of looping.)
  const [lastCurrentFile, setLastCurrentFile] = useState(operation.currentFileName)
  if (operation.currentFileName && operation.currentFileName !== lastCurrentFile) {
    setLastCurrentFile(operation.currentFileName)
  }
  const displayedFileName = showCurrentFile ? lastCurrentFile : null

  // Show the chart's smoothed leading-edge rate (not the raw instantaneous one)
  // so the number matches the curve and stays calm; fall back before any history.
  const currentRate =
    throughputHistory.length > 0
      ? throughputHistory[throughputHistory.length - 1].rate
      : operation.bytesPerSecond
  const liveView = useMemo(
    () => ({ operation, rate: currentRate, samples: throughputHistory, peak: throughputPeak }),
    [currentRate, operation, throughputHistory, throughputPeak],
  )
  // Throttle the live view (numbers + chart) to a calm, fixed cadence so they
  // update together a few times per second — like the Windows copy dialog —
  // rather than on every backend event.
  const [view, setView] = useState({ ...liveView, scanning: false })
  const latestViewRef = useRef(liveView)
  const lastTotalBytesRef = useRef(operation.totalBytes)
  useEffect(() => {
    latestViewRef.current = liveView
  })
  useEffect(() => {
    const intervalId = window.setInterval(() => {
      const latest = latestViewRef.current
      const scanning = latest.operation.totalBytes > lastTotalBytesRef.current
      lastTotalBytesRef.current = latest.operation.totalBytes
      setView({ ...latest, scanning })
    }, METRICS_REFRESH_MS)
    return () => window.clearInterval(intervalId)
  }, [])

  const displayed = view.operation
  // While a job is actively working through its queue, "N completed" reads as
  // "item 0 of 2" for the entire time the first item is copying — show the item
  // currently being worked on instead (like Windows' "item 1 of 2"). Once the
  // job stops (done/failed/cancelled), fall back to the true completed count.
  const displayedItemNumber = isFinished
    ? displayed.completedItems
    : Math.min(displayed.completedItems + 1, displayed.totalItems)
  const progressSegments = useMemo(
    () => formatJobProgress(displayed, { scanning: view.scanning }),
    [displayed, view.scanning],
  )
  const summary = progressSegments.join(' · ')
  const itemIndex = `item ${formatCount(displayedItemNumber)} of ${formatCount(displayed.totalItems)}`
  const rate = isPaused ? '—' : formatRate(view.rate)

  const announcement = progressSegments.join(', ')
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
      const nextAnnouncement = pendingLiveAnnouncementRef.current
      pendingLiveAnnouncementRef.current = null
      if (nextAnnouncement !== null && nextAnnouncement !== liveAnnouncementRef.current) {
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

  const title = isCompleted
    ? `${verb(operation)} complete`
    : isFailed
      ? `${verb(operation)} failed`
      : isCancelled
        ? `${verb(operation)} cancelled`
        : `${verb(operation)} ${formatCount(operation.totalItems)} items`
  const preview =
    formatItemPreview(operation.itemNames, operation.totalItems) ?? operation.sourceDir
  // Where the items go; a delete has no destination, so it names what it removes.
  const where = isPending
    ? `Waiting · ${preview}`
    : operation.destinationDir
      ? `to ${operation.destinationDir}`
      : preview

  return (
    <article
      aria-labelledby={headingId}
      data-status={operation.status}
      className="grid gap-2 px-4 py-3.5"
    >
      <div className="flex min-w-0 items-center gap-1.5">
        {reorderable ? (
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
        ) : null}
        {isCompleted ? (
          <CheckCircleIcon className="size-4 shrink-0 text-light-success dark:text-dark-success" />
        ) : null}
        {isFailed ? (
          <XCircleIcon className="size-4 shrink-0 text-light-danger dark:text-dark-danger" />
        ) : null}
        {isCancelled ? (
          <XCircleIcon className="size-4 shrink-0 text-light-text-secondary dark:text-dark-text-secondary" />
        ) : null}
        <span
          id={headingId}
          className="min-w-0 truncate text-body font-semibold text-light-text dark:text-dark-text"
        >
          {title}
        </span>
        {listedNames.length > 0 ? (
          <Button
            variant="subtle"
            aria-expanded={itemsExpanded}
            aria-controls={itemListId}
            aria-label={itemsExpanded ? 'Hide items' : 'Show items'}
            title={itemsExpanded ? 'Hide items' : 'Show items'}
            onClick={() => setItemsExpanded((expanded) => !expanded)}
            size="compact"
          >
            {itemsExpanded ? (
              <ChevronUpIcon className="size-3" />
            ) : (
              <ChevronDownIcon className="size-3" />
            )}
          </Button>
        ) : null}
      </div>

      <div className="flex min-w-0 justify-between gap-3 text-caption text-light-text-secondary dark:text-dark-text-secondary">
        <span className="min-w-0 truncate" title={where}>
          {where}
        </span>
        {showProgress && !isPending ? (
          <span
            className={`shrink-0 tabular-nums ${isFailed ? 'text-light-danger dark:text-dark-danger' : ''}`}
          >
            {roundedPercent}%
          </span>
        ) : null}
      </div>

      {itemsExpanded ? (
        <ul
          id={itemListId}
          className="flex max-h-queue-items flex-col gap-0.5 overflow-y-auto rounded-control bg-light-recessed px-2.5 py-1.5 text-caption text-light-text-secondary scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:bg-dark-recessed dark:text-dark-text-secondary dark:scrollbar-thumb-dark-text-tertiary"
        >
          {listedNames.map((name, index) => {
            const done = index < displayed.completedItems
            const current = !isFinished && index === displayed.completedItems
            return (
              <li
                key={name}
                className={`flex items-center gap-1 truncate ${
                  done
                    ? 'text-light-text-tertiary dark:text-dark-text-tertiary'
                    : current
                      ? 'text-light-text dark:text-dark-text'
                      : ''
                }`}
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
      ) : null}

      <span className="sr-only" aria-live="polite" aria-atomic="true">
        {liveAnnouncement}
      </span>

      {showProgress && !isPending ? (
        <>
          <ProgressBar
            value={percent}
            labelledBy={headingId}
            tone={isPaused ? 'neutral' : 'accent'}
          />
          <div className="flex min-w-0 justify-between gap-3 text-caption tabular-nums text-light-text-secondary dark:text-dark-text-secondary">
            <span className="min-w-0 truncate">{summary}</span>
            {showRate ? <span className="shrink-0">{rate}</span> : null}
          </div>
        </>
      ) : null}

      {isFailed && operation.errorMessage ? (
        <div className="rounded-control bg-light-danger-bg px-3 py-2 text-caption text-light-danger dark:bg-dark-danger-bg dark:text-dark-danger">
          {operation.errorMessage}
        </div>
      ) : isCancelled ? (
        <div className="text-caption text-light-text-secondary dark:text-dark-text-secondary">
          Job cancelled. Any completed file changes were kept.
        </div>
      ) : showChart ? (
        <ThroughputChart
          samples={view.samples}
          currentPercent={Math.min(100, Math.max(0, displayed.progressPercent))}
          peakRate={view.peak}
        />
      ) : null}

      {showCurrentFile ? (
        <div className="flex min-w-0 justify-between gap-3 text-caption tabular-nums text-light-text-tertiary dark:text-dark-text-tertiary">
          <span className="min-w-0 truncate" title={displayedFileName ?? undefined}>
            {displayedFileName ?? ' '}
          </span>
          <span className="shrink-0">{itemIndex}</span>
        </div>
      ) : null}

      <div className="mt-1 flex flex-wrap items-center gap-2">
        {isConflict ? (
          <Button variant="primary" onClick={onResolve}>
            Resolve conflict
          </Button>
        ) : isFailed ? (
          <>
            <Button onClick={onRetry}>
              <RotateCcwIcon className="size-4" /> Retry
            </Button>
            <Button variant="subtle" onClick={onDismiss}>
              Dismiss
            </Button>
          </>
        ) : isCompleted || isCancelled ? (
          <Button onClick={onDismiss}>Dismiss</Button>
        ) : isPaused ? (
          <Button onClick={onResume}>
            <PlayIcon className="size-4" /> Resume
          </Button>
        ) : isPending ? null : (
          <Button onClick={onPause}>
            <PauseIcon className="size-4" /> Pause
          </Button>
        )}

        {!isFinished && !isPending ? (
          <Button onClick={onSkip} disabled={!isPaused}>
            <SkipForwardIcon className="size-4" /> Skip
          </Button>
        ) : null}

        {!isFinished ? (
          <Button variant="subtle" onClick={onCancel}>
            Cancel
          </Button>
        ) : null}

        {reorderable && onMoveDown ? (
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
    </article>
  )
}
