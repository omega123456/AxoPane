import { describe, expect, it } from 'vitest'
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
} from '@/components/queue/throughput-tween'

const FIRST = { percent: 0, rate: 10 }
const SECOND = { percent: 50, rate: 30 }

describe('throughput tween', () => {
  it('eases and interpolates between endpoints', () => {
    expect(easeInOutCubic(0)).toBe(0)
    expect(easeInOutCubic(0.25)).toBeCloseTo(0.0625)
    expect(easeInOutCubic(1)).toBe(1)
    expect(easeOutCubic(1)).toBe(1)
    expect(interpolate(10, 30, 0.5)).toBe(20)
  })

  it('clamps animation progress and snaps under reduced motion', () => {
    expect(animationProgress(240, 480, false)).toBe(0.5)
    expect(animationProgress(960, 480, false)).toBe(1)
    expect(animationProgress(-5, 480, false)).toBe(0)
    expect(animationProgress(0, 480, true)).toBe(1)
  })

  it('draws an appended point in from the previous point', () => {
    const settled = settledTail([FIRST])
    expect(settled).toMatchObject({ start: null, progress: 1, identity: tailIdentity([FIRST]) })

    const appended = tailForTarget(settled, [FIRST, SECOND])
    expect(appended).toMatchObject({ start: FIRST, end: SECOND, progress: 0 })
    expect(tweenedSamples([FIRST, SECOND], appended)).toEqual([FIRST, FIRST])

    const halfway = advancedTail(appended, appended.identity, 0.5)
    const [, tweened] = tweenedSamples([FIRST, SECOND], halfway)
    expect(tweened.percent).toBeGreaterThan(FIRST.percent)
    expect(tweened.percent).toBeLessThan(SECOND.percent)

    const finished = advancedTail(halfway, appended.identity, 1)
    expect(finished.start).toBeNull()
    expect(tweenedSamples([FIRST, SECOND], finished)).toEqual([FIRST, SECOND])
    expect(advancedTail(halfway, 'stale', 1)).toBe(halfway)
  })

  it('renders non-append changes and empty targets straight from the target', () => {
    const shrunk = tailForTarget(settledTail([FIRST, SECOND]), [SECOND])
    expect(shrunk.start).toBeNull()
    expect(tweenedSamples([], shrunk)).toEqual([])
  })
})
