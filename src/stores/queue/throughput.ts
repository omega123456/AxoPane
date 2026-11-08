import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'
import { clampPercent, percentToFraction } from '@/lib/numbers'

export type ThroughputTrack = {
  history: ThroughputSample[]
  peak: number
  lastRawRate: number
  smoothedRate: number
}

type FoldedSample = Omit<ThroughputTrack, 'peak'>

export const THROUGHPUT_BUCKETS = 90
export const THROUGHPUT_EMA_FACTOR = 0.25

export const CLEARED_THROUGHPUT_TRACK: ThroughputTrack = {
  history: [],
  peak: 0,
  lastRawRate: 0,
  smoothedRate: 0,
}

function bucketOf(percent: number): number {
  const fraction = percentToFraction(clampPercent(percent))
  return Math.min(THROUGHPUT_BUCKETS - 1, Math.floor(fraction * THROUGHPUT_BUCKETS))
}

function crossesIntoNewBucket(previousPercent: number, percent: number) {
  return bucketOf(percent) > bucketOf(previousPercent)
}

function latestRate(history: ThroughputSample[]) {
  return history.at(-1)?.rate ?? 0
}

function withPeak(folded: FoldedSample, previousPeak: number): ThroughputTrack {
  return { ...folded, peak: Math.max(previousPeak, latestRate(folded.history)) }
}

function firstSample(progress: OpProgress): FoldedSample {
  const rate = progress.bytesPerSecond
  return {
    history: [{ percent: progress.progressPercent, rate }],
    lastRawRate: rate,
    smoothedRate: rate,
  }
}

function steppedSmoothedRate(track: ThroughputTrack, rate: number) {
  const backendRepeatedHeldRate = rate === track.lastRawRate
  if (backendRepeatedHeldRate) {
    return track.smoothedRate
  }
  return track.smoothedRate + (rate - track.smoothedRate) * THROUGHPUT_EMA_FACTOR
}

function foldSample(track: ThroughputTrack | undefined, progress: OpProgress): FoldedSample {
  if (!track) {
    return firstSample(progress)
  }
  const hasHistory = track.history.length > 0
  if (!hasHistory) {
    return firstSample(progress)
  }

  const percent = progress.progressPercent
  const rate = progress.bytesPerSecond
  const smoothedRate = steppedSmoothedRate(track, rate)
  const lastSample = track.history[track.history.length - 1]
  if (!crossesIntoNewBucket(lastSample.percent, percent)) {
    return { history: track.history, lastRawRate: rate, smoothedRate }
  }
  return {
    history: [...track.history, { percent, rate: smoothedRate }],
    lastRawRate: rate,
    smoothedRate,
  }
}

export function freshThroughputTrack(progress: OpProgress): ThroughputTrack {
  return withPeak(firstSample(progress), 0)
}

export function foldedThroughputTrack(
  track: ThroughputTrack | undefined,
  progress: OpProgress,
): ThroughputTrack {
  return withPeak(foldSample(track, progress), track?.peak ?? 0)
}

export function hydratedThroughputTrack(
  track: ThroughputTrack | undefined,
  progress: OpProgress,
): ThroughputTrack {
  if (!track) {
    return freshThroughputTrack(progress)
  }
  return withPeak(track, track.peak)
}

export function restartsThroughput(previous: OpProgress | undefined, next: OpProgress): boolean {
  if (!previous) {
    return false
  }

  return (
    next.progressPercent < previous.progressPercent ||
    next.copiedBytes < previous.copiedBytes ||
    next.completedItems < previous.completedItems ||
    next.status === 'pending'
  )
}

export function throughputTrackOf(
  throughput: Readonly<Record<string, ThroughputTrack>>,
  operationId: string,
): ThroughputTrack {
  return throughput[operationId] ?? CLEARED_THROUGHPUT_TRACK
}
