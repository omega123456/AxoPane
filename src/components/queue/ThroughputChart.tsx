import { curveMonotoneX } from '@visx/curve'
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
const MIN_CURVE_SAMPLES = 3
/** Tiny headroom so the peak point doesn't sit flush against the top edge. */
const CEILING_HEADROOM = 1.05

function clampPercent(percent: number) {
  return Math.min(100, Math.max(0, percent))
}

function normalizeCoordinate(value: number) {
  return Number(value.toFixed(3))
}

function normalizeSamples(samples: ThroughputSample[], currentPercent: number): ThroughputSample[] {
  const clampedCurrentPercent = clampPercent(currentPercent)
  const normalized = samples.map((sample) => ({
    percent: clampPercent(sample.percent),
    rate: Math.max(0, sample.rate),
  }))

  const fallbackRate = normalized.at(-1)?.rate ?? 0
  const currentSample = { percent: clampedCurrentPercent, rate: fallbackRate }

  if (normalized.length === 0) {
    return [currentSample]
  }

  const lastSample = normalized.at(-1)
  if (!lastSample) {
    return [currentSample]
  }

  if (lastSample.percent === clampedCurrentPercent) {
    return normalized.slice(0, -1).concat({
      percent: clampedCurrentPercent,
      rate: lastSample.rate,
    })
  }

  return normalized.concat(currentSample)
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

function buildChartSamples(samples: ThroughputSample[], currentPercent: number): ThroughputSample[] {
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
  const normalizedSamples = normalizeSamples(samples, clampedCurrentPercent)
  const chartSamples = buildChartSamples(samples, clampedCurrentPercent)
  const latestSample = chartSamples.at(-1) ?? { percent: clampedCurrentPercent, rate: 0 }
  // Frozen ceiling from the non-decreasing session peak — never recomputed from
  // the current samples, so the scale (and the whole curve) holds still.
  const ceilingRate = Math.max(1, peakRate * CEILING_HEADROOM)
  const xScale = scaleLinear<number>({
    domain: [0, 100],
    range: [0, VIEWBOX_WIDTH],
  })
  const yScale = scaleLinear<number>({
    domain: [0, ceilingRate],
    range: [BASELINE_Y, CEILING_Y],
  })
  // Defensive: a reading above the frozen ceiling pegs the top rather than
  // forcing a rescale (the ceiling only ever ratchets up between renders).
  const scaleRate = (rate: number) => yScale(Math.min(rate, ceilingRate))
  const showCurve = normalizedSamples.length >= MIN_CURVE_SAMPLES
  const clipId = `throughput-chart-fill-${chartSamples.length}-${Math.round(
    clampedCurrentPercent * 100,
  )}-${Math.round(latestSample.rate)}`
  const currentX = normalizeCoordinate(xScale(clampedCurrentPercent))
  const dotLeftPercent = normalizeCoordinate(latestSample.percent)
  const dotTopPercent = normalizeCoordinate(
    (scaleRate(latestSample.rate) / VIEWBOX_HEIGHT) * 100,
  )

  return (
    <div className="relative w-full">
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
          x1={currentX}
          x2={VIEWBOX_WIDTH}
          y1={BASELINE_Y}
          y2={BASELINE_Y}
          className="stroke-light-border dark:stroke-dark-border"
          strokeWidth="1"
          vectorEffect="non-scaling-stroke"
        />

        <rect
          data-testid="throughput-chart-progress-band"
          x="0"
          y={BASELINE_Y - 5}
          width={currentX}
          height="5"
          rx="2.5"
          className="fill-accent-blue-light/18 dark:fill-accent-blue/22"
        />

        {showCurve ? (
          <>
            <AreaClosed
              data={chartSamples}
              x={(sample) => xScale(sample.percent)}
              y={(sample) => scaleRate(sample.rate)}
              yScale={yScale}
              curve={curveMonotoneX}
              clipPath={`url(#${clipId})`}
              className="fill-accent-blue-light/15 dark:fill-accent-blue/20"
              data-testid="throughput-chart-area"
            />
            <LinePath
              data={chartSamples}
              x={(sample) => xScale(sample.percent)}
              y={(sample) => scaleRate(sample.rate)}
              curve={curveMonotoneX}
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
            className="stroke-accent-blue-light/35 dark:stroke-accent-blue/40"
            strokeWidth="1.5"
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>

      <span
        data-testid="throughput-chart-ceiling-label"
        className="pointer-events-none absolute left-0 top-0 font-mono text-uxs leading-none text-light-text-faint dark:text-dark-text-faint"
      >
        {formatRate(peakRate)}
      </span>

      <span
        data-testid="throughput-chart-dot"
        style={{ left: `${dotLeftPercent}%`, top: `${dotTopPercent}%` }}
        className="pointer-events-none absolute h-2 w-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent-blue-light ring-1 ring-light-window transition-all duration-500 ease-out motion-reduce:transition-none dark:bg-accent-blue dark:ring-dark-window"
      />
    </div>
  )
}
