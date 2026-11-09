import { useEffect, useMemo, useRef, useState } from 'react'
import { formatCount } from '@/lib/format'
import { formatJobProgress, type JobFlags } from '@/lib/queue-format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'
import {
  announcementToPublish,
  isDiscoveringBytes,
  retainedCurrentFile,
} from '@/components/queue/job-view-state'

const LIVE_REGION_THROTTLE_MS = 5000
/**
 * Cadence the visible metrics refresh at. The backend reports a jittery
 * instantaneous rate many times a second; sampling it on a calm interval keeps
 * the numbers readable (and roughly in step with the averaged chart) instead of
 * flickering on every progress event. The same tick decides whether the backend
 * is still discovering bytes, by comparing successive totals.
 */
const METRICS_REFRESH_MS = 500

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

export type LiveJobView = {
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

export type JobThroughput = {
  history: ThroughputSample[]
  peak: number
}

export function useJobCardView(operation: OpProgress, throughput: JobThroughput, flags: JobFlags) {
  const { history: throughputHistory, peak: throughputPeak } = throughput
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
  return { view, progressSegments, itemIndex, liveAnnouncement, lastCurrentFile }
}
