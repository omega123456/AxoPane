import type { ThroughputSample } from '@/lib/types/ipc'
import { clamp } from '@/lib/numbers'

export type TailTween = {
  identity: string
  length: number
  start: ThroughputSample | null
  end: ThroughputSample | null
  endKey: string
  progress: number
}

const CUBIC_POWER = 3
const EASE_IN_OUT_HALVES = 2
const EASE_IN_OUT_MIDPOINT = 1 / EASE_IN_OUT_HALVES
const SECOND_TO_LAST_INDEX = -2
const TWEEN_STARTED = 0
const TWEEN_FINISHED = 1
const NO_SAMPLE_KEY = ''

function cube(value: number) {
  return value ** CUBIC_POWER
}

export function easeInOutCubic(progress: number) {
  if (progress < EASE_IN_OUT_MIDPOINT) {
    return cube(progress * EASE_IN_OUT_HALVES) / EASE_IN_OUT_HALVES
  }
  return 1 - cube(EASE_IN_OUT_HALVES - progress * EASE_IN_OUT_HALVES) / EASE_IN_OUT_HALVES
}

export function easeOutCubic(progress: number) {
  return 1 - cube(1 - progress)
}

export function interpolate(start: number, end: number, progress: number) {
  return start + (end - start) * progress
}

export function animationProgress(elapsedMs: number, durationMs: number, reducedMotion: boolean) {
  if (reducedMotion) {
    return TWEEN_FINISHED
  }
  return clamp(elapsedMs / durationMs, TWEEN_STARTED, TWEEN_FINISHED)
}

function sampleKey(sample: ThroughputSample | null) {
  return sample ? `${sample.percent}:${sample.rate}` : NO_SAMPLE_KEY
}

export function tailIdentity(target: ThroughputSample[]): string {
  return `${target.length}|${sampleKey(target.at(-1) ?? null)}`
}

export function settledTail(target: ThroughputSample[]): TailTween {
  const end = target.at(-1) ?? null
  return {
    identity: tailIdentity(target),
    length: target.length,
    start: null,
    end,
    endKey: sampleKey(end),
    progress: TWEEN_FINISHED,
  }
}

function appendedPoint(previous: TailTween, target: ThroughputSample[]) {
  const end = target.at(-1) ?? null
  const previousPoint = target.at(SECOND_TO_LAST_INDEX) ?? null
  const grew = target.length > previous.length
  const appended = end !== null && grew
  return appended ? previousPoint : null
}

export function tailForTarget(previous: TailTween, target: ThroughputSample[]): TailTween {
  const start = appendedPoint(previous, target)
  if (!start) {
    return settledTail(target)
  }
  return { ...settledTail(target), start, progress: TWEEN_STARTED }
}

export function advancedTail(previous: TailTween, identity: string, progress: number): TailTween {
  if (previous.identity !== identity) {
    return previous
  }
  const finished = progress >= TWEEN_FINISHED
  return { ...previous, progress, start: finished ? null : previous.start }
}

type ActiveTailTween = TailTween & { start: ThroughputSample; end: ThroughputSample }

function isTweening(tail: TailTween, end: ThroughputSample): tail is ActiveTailTween {
  const tweenActive = tail.start !== null && tail.end !== null && tail.progress < TWEEN_FINISHED
  return tweenActive && tail.endKey === sampleKey(end)
}

export function tweenedSamples(target: ThroughputSample[], tail: TailTween): ThroughputSample[] {
  const end = target.at(-1)
  if (!end) {
    return target
  }
  if (!isTweening(tail, end)) {
    return target
  }
  const eased = easeOutCubic(tail.progress)
  return [
    ...target.slice(0, -1),
    {
      percent: interpolate(tail.start.percent, tail.end.percent, eased),
      rate: interpolate(tail.start.rate, tail.end.rate, eased),
    },
  ]
}
