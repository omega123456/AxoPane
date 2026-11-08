import { useEffect, useMemo, useRef, useState } from 'react'
import { curveLinear } from '@visx/curve'
import { scaleLinear } from '@visx/scale'
import { AreaClosed, LinePath } from '@visx/shape'
import type { ThroughputSample } from '@/lib/types/ipc'
import { clampPercent, PERCENT_SCALE } from '@/lib/numbers'
import scaleSteps from './throughput-scale-steps.json'
import {
  advancedTail,
  animationProgress,
  easeInOutCubic,
  easeOutCubic,
  interpolate,
  settledTail,
  tailForTarget,
  tailIdentity,
  tweenedSamples,
  type TailTween,
} from './throughput-tween'

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
const BASELINE_INSET = 2
const BASELINE_Y = VIEWBOX_HEIGHT - BASELINE_INSET
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
/** Rounded mantissas the ceiling snaps up to within each power of ten. */
const SCALE_STEPS: readonly number[] = scaleSteps
const DECIMAL_BASE = 10
const COORDINATE_PRECISION_DIGITS = 3
const CLIP_ID_PERCENT_PRECISION = 100
const SCALE_SETTLED_TOLERANCE = 0.5
const SECOND_TO_LAST_INDEX = -2
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

function normalizeCoordinate(value: number) {
  return Number(value.toFixed(COORDINATE_PRECISION_DIGITS))
}

function niceCeilingRate(rate: number) {
  const rateWithHeadroom = Math.max(1, rate * CEILING_HEADROOM)
  const magnitude = DECIMAL_BASE ** Math.floor(Math.log10(rateWithHeadroom))
  const normalized = rateWithHeadroom / magnitude
  const step = SCALE_STEPS.find((candidate) => normalized <= candidate) ?? DECIMAL_BASE
  return step * magnitude
}

function prefersReducedMotion() {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
}

function cancelFrame(animationRef: { current: number | null }) {
  if (animationRef.current !== null) {
    window.cancelAnimationFrame(animationRef.current)
    animationRef.current = null
  }
}

function useAppendedTail(target: ThroughputSample[]): {
  samples: ThroughputSample[]
  settled: boolean
} {
  const identity = tailIdentity(target)
  const [tail, setTail] = useState<TailTween>(() => settledTail(target))
  const animationRef = useRef<number | null>(null)

  if (tail.identity !== identity) {
    setTail(tailForTarget(tail, target))
  }

  useEffect(() => {
    if (tail.start === null) {
      return
    }

    const reducedMotion = prefersReducedMotion()
    const startedAt = performance.now()
    const tick = (time: number) => {
      const progress = animationProgress(time - startedAt, APPEND_ANIMATION_MS, reducedMotion)
      setTail((previous) => advancedTail(previous, tail.identity, progress))
      animationRef.current = progress < 1 ? window.requestAnimationFrame(tick) : null
    }

    cancelFrame(animationRef)
    animationRef.current = window.requestAnimationFrame(tick)
    return () => cancelFrame(animationRef)
  }, [tail.identity, tail.start])

  const samples = useMemo(() => tweenedSamples(target, tail), [target, tail])
  return { samples, settled: tail.start === null }
}

function useEasedScalar(target: number, durationMs: number, easing: (progress: number) => number) {
  const [displayed, setDisplayed] = useState(target)
  const displayedRef = useRef(target)
  const animationRef = useRef<number | null>(null)

  useEffect(() => {
    displayedRef.current = displayed
  }, [displayed])

  useEffect(() => {
    const startValue = displayedRef.current
    if (startValue === target) {
      return
    }

    const reducedMotion = prefersReducedMotion()
    const startedAt = performance.now()
    const tick = (time: number) => {
      const progress = animationProgress(time - startedAt, durationMs, reducedMotion)
      const finished = progress >= 1
      const nextValue = finished ? target : interpolate(startValue, target, easing(progress))
      displayedRef.current = nextValue
      setDisplayed(nextValue)
      animationRef.current = finished ? null : window.requestAnimationFrame(tick)
    }

    cancelFrame(animationRef)
    animationRef.current = window.requestAnimationFrame(tick)
    return () => cancelFrame(animationRef)
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

  const previous = normalized.at(SECOND_TO_LAST_INDEX)
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

function useThroughputChartModel({ samples, currentPercent, peakRate }: ThroughputChartProps) {
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
    domain: [0, PERCENT_SCALE],
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
    clampedCurrentPercent * CLIP_ID_PERCENT_PRECISION,
  )}-${Math.round(committedSamples.at(-1)?.rate ?? 0)}`
  // The progress fill and leading edge follow the eased percent, so the edge
  // glides while the committed trace behind it holds still.
  const currentX = normalizeCoordinate(xScale(displayedCurrentPercent))
  const unfilledWidth = normalizeCoordinate(VIEWBOX_WIDTH - currentX)
  // Settled only once the rescale *and* the tail draw-in have finished, so a
  // screenshot wait lands on a stable frame.
  const chartSettled =
    Math.abs(displayedCeilingRate - ceilingRate) < SCALE_SETTLED_TOLERANCE && tailSettled
  return {
    chartSamples,
    xScale,
    yScale,
    scaleRate,
    showCurve,
    clipId,
    currentX,
    unfilledWidth,
    chartSettled,
  }
}

type ChartModel = ReturnType<typeof useThroughputChartModel>

function ChartBackdrop({ clipId, currentX, unfilledWidth }: ChartModel) {
  return (
    <>
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
        className="fill-light-accent/10 dark:fill-dark-accent/10"
      />

      <rect
        data-testid="throughput-chart-unfilled"
        x={currentX}
        y="0"
        width={unfilledWidth}
        height={VIEWBOX_HEIGHT}
        className="fill-light-card/60 dark:fill-dark-layer/45"
      />

      <ChartGuides />
    </>
  )
}

function ChartGuides() {
  return (
    <>
      <line
        data-testid="throughput-chart-ceiling"
        x1="0"
        x2={VIEWBOX_WIDTH}
        y1={CEILING_Y}
        y2={CEILING_Y}
        className="stroke-light-divider dark:stroke-dark-divider"
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
        className="stroke-light-divider dark:stroke-dark-divider"
        strokeWidth="1"
        vectorEffect="non-scaling-stroke"
      />
    </>
  )
}

function ChartTrace({ chartSamples, xScale, yScale, scaleRate, showCurve, clipId }: ChartModel) {
  if (!showCurve) {
    return (
      <line
        data-testid="throughput-chart-baseline"
        x1="0"
        x2={VIEWBOX_WIDTH}
        y1={BASELINE_Y}
        y2={BASELINE_Y}
        clipPath={`url(#${clipId})`}
        className="stroke-light-accent/35 dark:stroke-dark-accent/40"
        strokeWidth="1.5"
        vectorEffect="non-scaling-stroke"
      />
    )
  }
  return (
    <>
      <AreaClosed
        data={chartSamples}
        x={(sample) => xScale(sample.percent)}
        y={(sample) => scaleRate(sample.rate)}
        yScale={yScale}
        curve={curveLinear}
        clipPath={`url(#${clipId})`}
        className="fill-light-accent/18 dark:fill-dark-accent/55"
        data-testid="throughput-chart-area"
      />
      <LinePath
        data={chartSamples}
        x={(sample) => xScale(sample.percent)}
        y={(sample) => scaleRate(sample.rate)}
        curve={curveLinear}
        clipPath={`url(#${clipId})`}
        className="stroke-light-accent dark:stroke-dark-accent"
        strokeWidth="1.5"
        fill="none"
        vectorEffect="non-scaling-stroke"
        data-testid="throughput-chart-line"
      />
    </>
  )
}

export function ThroughputChart(props: ThroughputChartProps) {
  const model = useThroughputChartModel(props)

  return (
    <div
      data-testid="throughput-chart"
      data-scale-settled={model.chartSettled ? 'true' : 'false'}
      className="relative w-full overflow-hidden rounded-control border border-light-stroke bg-light-skeleton dark:border-dark-stroke dark:bg-dark-skeleton"
    >
      <svg
        aria-hidden="true"
        viewBox={`0 0 ${VIEWBOX_WIDTH} ${VIEWBOX_HEIGHT}`}
        preserveAspectRatio="none"
        className="block h-queue-chart w-full"
      >
        <ChartBackdrop {...model} />
        <ChartTrace {...model} />
      </svg>
      <span
        data-testid="throughput-chart-leading-edge"
        style={{ left: `${model.currentX}%` }}
        className="pointer-events-none absolute inset-y-0 w-px bg-light-accent/45 dark:bg-dark-accent/45"
      />
    </div>
  )
}
