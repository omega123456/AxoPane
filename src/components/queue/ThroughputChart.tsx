import { useEffect, useMemo, useRef, useState } from 'react'
import { curveLinear } from '@visx/curve'
import { scaleLinear } from '@visx/scale'
import { AreaClosed, LinePath } from '@visx/shape'
import { formatRate } from '@/lib/format'
import type { ThroughputSample } from '@/lib/types/ipc'

type ThroughputChartProps = {
  samples: ThroughputSample[]
  currentPercent: number
  /**
   * The frozen Y-axis maximum: the highest averaged speed seen so far this
   * transfer (non-decreasing). Like the Windows copy dialog, the scale is set
   * by the early burst and never recalculated downward — the line simply
   * descends as speed drops, so the chart never rescales/jumps under the curve.
   */
  peakRate: number
}

const VIEWBOX_WIDTH = 100
const VIEWBOX_HEIGHT = 48
const BASELINE_Y = VIEWBOX_HEIGHT - 2
const CEILING_Y = 8
/**
 * Draw the trace as soon as there are two committed points (plus the left
 * anchor). With finer progress buckets in the store this lands within the first
 * ~1% of a transfer, so a huge copy shows a speed line almost immediately rather
 * than sitting on the baseline until several buckets have filled.
 */
const MIN_CURVE_SAMPLES = 2
/**
 * Leave generous room above the measured peak. The chart should feel like a
 * transfer progress surface, not a live oscilloscope that rescales whenever
 * the backend reports a slightly better burst.
 */
const CEILING_HEADROOM = 1.35
const SCALE_STEPS = [1, 1.25, 1.6, 2, 2.5, 3.2, 4, 5, 6.4, 8, 10] as const
const CHART_ANIMATION_MS = 900
/**
 * Duration of the subtle "draw-in" when a new committed point appends. Only the
 * new tail segment eases from the previous point to its final spot; every point
 * behind it stays frozen, so this refines the append without re-introducing the
 * wandering leading point.
 */
const APPEND_ANIMATION_MS = 480
/**
 * Duration of the progress-edge ease — the fill extent and the leading vertical
 * line glide toward the live percent over this window instead of teleporting, so
 * a big speed/progress jump sweeps across rather than snapping. Matches the tail
 * draw-in so the fill reveals the curve as it is drawn.
 */
const PROGRESS_EASE_MS = 480

function clampPercent(percent: number) {
  return Math.min(100, Math.max(0, percent))
}

function normalizeCoordinate(value: number) {
  return Number(value.toFixed(3))
}

function niceCeilingRate(rate: number) {
  const rateWithHeadroom = Math.max(1, rate * CEILING_HEADROOM)
  const magnitude = 10 ** Math.floor(Math.log10(rateWithHeadroom))
  const normalized = rateWithHeadroom / magnitude
  const step = SCALE_STEPS.find((candidate) => normalized <= candidate) ?? 10
  return step * magnitude
}

function easeInOutCubic(progress: number) {
  return progress < 0.5 ? 4 * progress ** 3 : 1 - (-2 * progress + 2) ** 3 / 2
}

function easeOutCubic(progress: number) {
  return 1 - (1 - progress) ** 3
}

function interpolate(start: number, end: number, progress: number) {
  return start + (end - start) * progress
}

function prefersReducedMotion() {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
}

function sampleKey(sample: ThroughputSample) {
  return `${sample.percent}:${sample.rate}`
}

type TailState = {
  /** Identity of the committed target (length + last point), to detect changes. */
  identity: string
  length: number
  /** Draw-in start point; null when nothing is easing in. */
  start: ThroughputSample | null
  end: ThroughputSample | null
  endKey: string
  progress: number
}

function tailIdentity(target: ThroughputSample[]): string {
  const end = target.at(-1)
  return `${target.length}|${end ? sampleKey(end) : ''}`
}

/**
 * Ease a freshly committed point into place. When the sample list grows, the new
 * tail point draws in from the previous (frozen) point over {@link
 * APPEND_ANIMATION_MS}; everything behind it is untouched. Any non-append change
 * (a held bucket, a rescale, a reset) renders straight from `target`, so the only
 * thing that ever moves is the single segment currently being drawn — and it
 * eases toward a fixed committed target, never a wandering one.
 *
 * State is seeded during render (a supported React pattern) so the first painted
 * frame of an append already sits at the start point — the segment grows in with
 * no full-length flash first. `settled` is false only while the draw-in runs, so
 * screenshot waits can hold for a stable frame.
 */
function useAppendedTail(target: ThroughputSample[]): {
  samples: ThroughputSample[]
  settled: boolean
} {
  const identity = tailIdentity(target)
  const [state, setState] = useState<TailState>(() => {
    const end = target.at(-1) ?? null
    return {
      identity,
      length: target.length,
      start: null,
      end,
      endKey: end ? sampleKey(end) : '',
      progress: 1,
    }
  })
  const startedAtRef = useRef(0)
  const animationRef = useRef<number | null>(null)

  if (state.identity !== identity) {
    const end = target.at(-1) ?? null
    const isAppend = end !== null && target.length > state.length && target.length >= 2
    setState({
      identity,
      length: target.length,
      start: isAppend ? (target[target.length - 2] ?? null) : null,
      end,
      endKey: end ? sampleKey(end) : '',
      progress: isAppend ? 0 : 1,
    })
  }

  useEffect(() => {
    // start is non-null only for a fresh append; it is cleared once progress hits
    // 1, so this is equivalent to "an animation is pending".
    if (state.start === null) {
      return
    }

    const reducedMotion = prefersReducedMotion()
    startedAtRef.current = performance.now()

    const tick = (time: number) => {
      const progress = reducedMotion
        ? 1
        : Math.min(1, Math.max(0, (time - startedAtRef.current) / APPEND_ANIMATION_MS))
      setState((previous) =>
        previous.identity === state.identity
          ? { ...previous, progress, start: progress >= 1 ? null : previous.start }
          : previous,
      )

      if (progress < 1) {
        animationRef.current = window.requestAnimationFrame(tick)
        return
      }
      animationRef.current = null
    }

    if (animationRef.current !== null) {
      window.cancelAnimationFrame(animationRef.current)
    }
    animationRef.current = window.requestAnimationFrame(tick)

    return () => {
      if (animationRef.current !== null) {
        window.cancelAnimationFrame(animationRef.current)
        animationRef.current = null
      }
    }
  }, [state.identity, state.start])

  const samples = useMemo(() => {
    if (target.length === 0) {
      return target
    }
    const end = target[target.length - 1]
    if (
      state.start !== null &&
      state.end !== null &&
      state.progress < 1 &&
      state.endKey === sampleKey(end)
    ) {
      const eased = easeOutCubic(state.progress)
      return [
        ...target.slice(0, -1),
        {
          percent: interpolate(state.start.percent, state.end.percent, eased),
          rate: interpolate(state.start.rate, state.end.rate, eased),
        },
      ]
    }
    return target
  }, [target, state.start, state.end, state.progress, state.endKey])

  return { samples, settled: state.start === null }
}

/**
 * Ease a single scalar toward `target`, re-aiming from wherever it currently sits
 * whenever `target` changes. Used for the Y-axis ceiling (so a rescale glides
 * instead of snapping) and for the progress edge (so a big speed/progress jump
 * sweeps the fill + leading line across smoothly rather than teleporting). The
 * sample geometry is never routed through this — committed points stay frozen.
 */
function useEasedScalar(target: number, durationMs: number, easing: (progress: number) => number) {
  const [displayed, setDisplayed] = useState(target)
  const displayedRef = useRef(target)
  const animationRef = useRef<number | null>(null)

  useEffect(() => {
    displayedRef.current = displayed
  }, [displayed])

  useEffect(() => {
    const startValue = displayedRef.current
    // Already there — nothing to animate.
    if (startValue === target) {
      return
    }

    // Reduced-motion users snap on the first frame (progress pinned to 1); the
    // single setState site stays inside the rAF callback, never the effect body.
    const reducedMotion = prefersReducedMotion()
    const startedAt = performance.now()

    const tick = (time: number) => {
      const progress = reducedMotion ? 1 : Math.min(1, Math.max(0, (time - startedAt) / durationMs))
      const nextValue = interpolate(startValue, target, easing(progress))
      displayedRef.current = nextValue
      setDisplayed(nextValue)

      if (progress < 1) {
        animationRef.current = window.requestAnimationFrame(tick)
        return
      }

      animationRef.current = null
      displayedRef.current = target
      setDisplayed(target)
    }

    if (animationRef.current !== null) {
      window.cancelAnimationFrame(animationRef.current)
    }
    animationRef.current = window.requestAnimationFrame(tick)

    return () => {
      if (animationRef.current !== null) {
        window.cancelAnimationFrame(animationRef.current)
        animationRef.current = null
      }
    }
  }, [target, durationMs, easing])

  return displayed
}

function normalizeSamples(samples: ThroughputSample[], currentPercent: number): ThroughputSample[] {
  const clampedCurrentPercent = clampPercent(currentPercent)
  const normalized = samples.map((sample) => ({
    percent: clampPercent(sample.percent),
    rate: Math.max(0, sample.rate),
  }))

  if (normalized.length === 0) {
    return [{ percent: clampedCurrentPercent, rate: 0 }]
  }

  return normalized
}

function anchorChartSamples(samples: ThroughputSample[]): ThroughputSample[] {
  const anchored: ThroughputSample[] = [{ percent: 0, rate: samples[0]?.rate ?? 0 }]

  for (const sample of samples) {
    const previous = anchored.at(-1)
    if (!previous) {
      anchored.push(sample)
      continue
    }

    if (sample.percent === previous.percent) {
      anchored[anchored.length - 1] = sample
      continue
    }

    anchored.push(sample)
  }

  return anchored
}

function buildChartSamples(
  samples: ThroughputSample[],
  currentPercent: number,
): ThroughputSample[] {
  const normalized = anchorChartSamples(normalizeSamples(samples, currentPercent))
  const lastSample = normalized.at(-1)

  if (!lastSample || lastSample.rate > 0) {
    return normalized
  }

  const previous = normalized.at(-2)
  if (!previous || previous.percent === lastSample.percent) {
    return normalized
  }

  return normalized.slice(0, -1).concat(
    {
      percent: previous.percent,
      rate: 0,
    },
    lastSample,
  )
}

export function ThroughputChart({ samples, currentPercent, peakRate }: ThroughputChartProps) {
  const clampedCurrentPercent = clampPercent(currentPercent)
  const normalizedSamples = useMemo(
    () => normalizeSamples(samples, clampedCurrentPercent),
    [clampedCurrentPercent, samples],
  )
  // Committed samples: passed buckets are frozen, so the trace only ever grows a
  // new segment to the right and never bends behind the leading edge.
  const committedSamples = useMemo(
    () => buildChartSamples(samples, clampedCurrentPercent),
    [clampedCurrentPercent, samples],
  )
  // Ease each freshly committed point into place; everything behind it is frozen.
  const { samples: chartSamples, settled: tailSettled } = useAppendedTail(committedSamples)
  // Frozen, banded ceiling from the non-decreasing session peak. The store
  // keeps the peak from dropping; this component rounds upward into stable
  // bands so small new highs do not suddenly compress the whole curve.
  const ceilingRate = niceCeilingRate(peakRate)
  const displayedCeilingRate = useEasedScalar(ceilingRate, CHART_ANIMATION_MS, easeInOutCubic)
  // Ease the progress edge so a jump sweeps the fill/leading line rather than
  // teleporting; the trace behind it still holds still.
  const displayedCurrentPercent = useEasedScalar(
    clampedCurrentPercent,
    PROGRESS_EASE_MS,
    easeOutCubic,
  )
  const xScale = scaleLinear<number>({
    domain: [0, 100],
    range: [0, VIEWBOX_WIDTH],
  })
  const yScale = scaleLinear<number>({
    domain: [0, displayedCeilingRate],
    range: [BASELINE_Y, CEILING_Y],
  })
  // Defensive: a reading above the frozen ceiling pegs the top rather than
  // forcing a rescale (the ceiling only ever ratchets up between renders).
  const scaleRate = (rate: number) => yScale(Math.min(rate, displayedCeilingRate))
  const showCurve = normalizedSamples.length >= MIN_CURVE_SAMPLES
  const clipId = `throughput-chart-fill-${committedSamples.length}-${Math.round(
    clampedCurrentPercent * 100,
  )}-${Math.round(committedSamples.at(-1)?.rate ?? 0)}`
  // The progress fill and leading edge follow the eased percent, so the edge
  // glides while the committed trace behind it holds still.
  const currentX = normalizeCoordinate(xScale(displayedCurrentPercent))
  const unfilledWidth = normalizeCoordinate(VIEWBOX_WIDTH - currentX)
  // Settled only once the rescale *and* the tail draw-in have finished, so a
  // screenshot wait lands on a stable frame.
  const chartSettled = Math.abs(displayedCeilingRate - ceilingRate) < 0.5 && tailSettled

  return (
    <div
      data-testid="throughput-chart"
      data-scale-settled={chartSettled ? 'true' : 'false'}
      className="relative w-full overflow-hidden rounded-tab border border-light-border bg-light-skeleton dark:border-dark-border dark:bg-dark-skeleton"
    >
      <svg
        aria-hidden="true"
        viewBox={`0 0 ${VIEWBOX_WIDTH} ${VIEWBOX_HEIGHT}`}
        preserveAspectRatio="none"
        className="block h-queue-chart w-full"
      >
        <defs>
          <clipPath id={clipId}>
            <rect
              data-testid="throughput-chart-fill-extent"
              x="0"
              y="0"
              width={currentX}
              height={VIEWBOX_HEIGHT}
            />
          </clipPath>
        </defs>

        <rect
          data-testid="throughput-chart-progress-fill"
          x="0"
          y="0"
          width={currentX}
          height={VIEWBOX_HEIGHT}
          className="fill-accent-blue-light/10 dark:fill-accent-blue/10"
        />

        <rect
          data-testid="throughput-chart-unfilled"
          x={currentX}
          y="0"
          width={unfilledWidth}
          height={VIEWBOX_HEIGHT}
          className="fill-light-surface/60 dark:fill-dark-surface/45"
        />

        <line
          data-testid="throughput-chart-ceiling"
          x1="0"
          x2={VIEWBOX_WIDTH}
          y1={CEILING_Y}
          y2={CEILING_Y}
          className="stroke-light-border dark:stroke-dark-border"
          strokeWidth="1"
          strokeDasharray="2 2"
          vectorEffect="non-scaling-stroke"
        />

        <line
          data-testid="throughput-chart-track"
          x1="0"
          x2={VIEWBOX_WIDTH}
          y1={BASELINE_Y}
          y2={BASELINE_Y}
          className="stroke-light-border dark:stroke-dark-border"
          strokeWidth="1"
          vectorEffect="non-scaling-stroke"
        />

        {showCurve ? (
          <>
            <AreaClosed
              data={chartSamples}
              x={(sample) => xScale(sample.percent)}
              y={(sample) => scaleRate(sample.rate)}
              yScale={yScale}
              curve={curveLinear}
              clipPath={`url(#${clipId})`}
              className="fill-accent-blue-light/18 dark:fill-accent-blue/55"
              data-testid="throughput-chart-area"
            />
            <LinePath
              data={chartSamples}
              x={(sample) => xScale(sample.percent)}
              y={(sample) => scaleRate(sample.rate)}
              curve={curveLinear}
              clipPath={`url(#${clipId})`}
              className="stroke-accent-blue-light dark:stroke-accent-blue"
              strokeWidth="1.5"
              fill="none"
              vectorEffect="non-scaling-stroke"
              data-testid="throughput-chart-line"
            />
          </>
        ) : (
          <line
            data-testid="throughput-chart-baseline"
            x1="0"
            x2={VIEWBOX_WIDTH}
            y1={BASELINE_Y}
            y2={BASELINE_Y}
            clipPath={`url(#${clipId})`}
            className="stroke-accent-blue-light/35 dark:stroke-accent-blue/40"
            strokeWidth="1.5"
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>
      <span
        data-testid="throughput-chart-scale-label"
        className="pointer-events-none absolute left-3 top-2 font-mono text-uxs leading-none text-light-text-faint dark:text-dark-text-faint"
      >
        {formatRate(ceilingRate)}
      </span>
      <span
        data-testid="throughput-chart-leading-edge"
        style={{ left: `${currentX}%` }}
        className="pointer-events-none absolute inset-y-0 w-px bg-accent-blue-light/45 dark:bg-accent-blue/45"
      />
    </div>
  )
}
