import { act, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import type { ThroughputSample } from '@/lib/types/ipc'

function sample(percent: number, rate: number): ThroughputSample {
  return { percent, rate }
}

function installAnimationFrameMock() {
  let id = 0
  const callbacks = new Map<number, FrameRequestCallback>()
  const requestSpy = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    id += 1
    callbacks.set(id, callback)
    return id
  })
  const cancelSpy = vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((handle) => {
    callbacks.delete(handle)
  })

  return {
    step(time: number) {
      const queued = [...callbacks.values()]
      callbacks.clear()
      for (const callback of queued) {
        callback(time)
      }
    },
    restore() {
      callbacks.clear()
      requestSpy.mockRestore()
      cancelSpy.mockRestore()
    },
  }
}

describe('ThroughputChart', () => {
  it('renders a filled progress surface with area, line, ceiling and leading edge', () => {
    render(
      <ThroughputChart
        samples={[sample(12, 120), sample(36, 220), sample(64, 180), sample(81, 260)]}
        currentPercent={81}
        peakRate={260}
      />,
    )

    const svg = document.querySelector('svg')
    expect(svg).toHaveAttribute('aria-hidden', 'true')
    expect(screen.getByTestId('throughput-chart-area')).toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-line')).toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-ceiling')).toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).not.toContain('C')
    expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '81')
    expect(screen.getByTestId('throughput-chart-progress-fill')).toHaveAttribute('width', '81')
    expect(screen.getByTestId('throughput-chart-unfilled')).toHaveAttribute('x', '81')
    expect(screen.getByTestId('throughput-chart-unfilled')).toHaveAttribute('width', '19')
    expect(screen.getByTestId('throughput-chart-track')).toHaveAttribute('x1', '0')
    expect(screen.getByTestId('throughput-chart-leading-edge').style.left).toBe('81%')
  })

  it('renders a baseline and leading edge without line or area when only one sample exists', () => {
    render(<ThroughputChart samples={[sample(18, 140)]} currentPercent={18} peakRate={140} />)

    expect(screen.getByTestId('throughput-chart-baseline')).toBeInTheDocument()
    expect(screen.queryByTestId('throughput-chart-area')).not.toBeInTheDocument()
    expect(screen.queryByTestId('throughput-chart-line')).not.toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-leading-edge').style.left).toBe('18%')
  })

  it('draws the trace from just two committed samples so a huge copy shows a line early', () => {
    render(
      <ThroughputChart
        samples={[sample(0.4, 140), sample(1.1, 180)]}
        currentPercent={1.1}
        peakRate={180}
      />,
    )

    expect(screen.getByTestId('throughput-chart-line')).toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-area')).toBeInTheDocument()
    // Anchored to the left edge so the early sliver still reads as a real trace.
    expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toContain('0,')
  })

  it('flatlines the leading edge when the current rate is zero', () => {
    render(
      <ThroughputChart
        samples={[sample(15, 160), sample(48, 210), sample(77, 0)]}
        currentPercent={77}
        peakRate={210}
      />,
    )

    const line = screen.getByTestId('throughput-chart-line')
    expect(line.getAttribute('d')).toContain('77,46')
    expect(screen.getByTestId('throughput-chart-leading-edge').style.left).toBe('77%')
  })

  it('anchors the chart to the left edge so late history still reads as determinate progress', () => {
    render(
      <ThroughputChart
        samples={[sample(55, 140), sample(60, 220), sample(63, 260)]}
        currentPercent={63}
        peakRate={260}
      />,
    )

    expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toContain('0,')
    expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '63')
  })

  it('uses the current percent as the fill extent even when the latest sample percent lags behind', () => {
    render(
      <ThroughputChart
        samples={[sample(20, 100), sample(40, 180), sample(58, 220)]}
        currentPercent={63}
        peakRate={220}
      />,
    )

    expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '63')
    expect(screen.getByTestId('throughput-chart-leading-edge').style.left).toBe('63%')
    const linePath = screen.getByTestId('throughput-chart-line').getAttribute('d')
    expect(linePath).toContain('L58,')
    expect(linePath).not.toContain('L63,')
  })

  it('eases the progress edge toward the new percent without inventing a speed trace point', () => {
    let now = 0
    const animationFrame = installAnimationFrameMock()
    const nowSpy = vi.spyOn(performance, 'now').mockImplementation(() => now)
    const steadySamples = [sample(20, 100), sample(40, 180), sample(58, 220)]

    try {
      const { rerender } = render(
        <ThroughputChart samples={steadySamples} currentPercent={58} peakRate={220} />,
      )
      const firstLine = screen.getByTestId('throughput-chart-line').getAttribute('d')
      expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '58')

      act(() => {
        rerender(<ThroughputChart samples={steadySamples} currentPercent={63} peakRate={220} />)
      })

      // The trace geometry is unchanged (no invented point) and the edge has not
      // teleported to 63 — it still sits at 58 until the ease runs.
      expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toBe(firstLine)
      expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '58')

      // Part-way through the ease the fill is between the old and new percent.
      act(() => {
        now = 240
        animationFrame.step(now)
      })
      const midWidth = Number(
        screen.getByTestId('throughput-chart-fill-extent').getAttribute('width'),
      )
      expect(midWidth).toBeGreaterThan(58)
      expect(midWidth).toBeLessThan(63)

      // Settled: the edge lands exactly on the new percent, trace still unchanged.
      act(() => {
        now = 480
        animationFrame.step(now)
      })
      expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '63')
      expect(screen.getByTestId('throughput-chart-leading-edge').style.left).toBe('63%')
      expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toBe(firstLine)
    } finally {
      nowSpy.mockRestore()
      animationFrame.restore()
    }
  })

  it('keeps the same scale band for small peak increases', () => {
    const steady = [sample(20, 250), sample(40, 250), sample(60, 250)]
    const { rerender } = render(
      <ThroughputChart samples={steady} currentPercent={60} peakRate={1000} />,
    )
    const firstLine = screen.getByTestId('throughput-chart-line').getAttribute('d')
    expect(screen.getByTestId('throughput-chart-scale-label')).toHaveTextContent('1.6 KB/s')

    // 1100 B/s is a real new high, but it still fits in the 1.6 KB/s band; the
    // chart should not compress just because the peak nudged upward.
    rerender(<ThroughputChart samples={steady} currentPercent={60} peakRate={1100} />)
    expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toBe(firstLine)
    expect(screen.getByTestId('throughput-chart-scale-label')).toHaveTextContent('1.6 KB/s')
  })

  it('animates into a higher scale band when the peak meaningfully outgrows the old one', () => {
    let now = 0
    const animationFrame = installAnimationFrameMock()
    const nowSpy = vi.spyOn(performance, 'now').mockImplementation(() => now)
    const steady = [sample(20, 250), sample(40, 250), sample(60, 250)]

    try {
      const { rerender } = render(
        <ThroughputChart samples={steady} currentPercent={60} peakRate={1000} />,
      )
      const firstLine = screen.getByTestId('throughput-chart-line').getAttribute('d')

      act(() => {
        rerender(<ThroughputChart samples={steady} currentPercent={60} peakRate={2000} />)
      })
      expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toBe(firstLine)
      expect(screen.getByTestId('throughput-chart-scale-label')).toHaveTextContent('3.1 KB/s')
      expect(screen.getByTestId('throughput-chart')).toHaveAttribute('data-scale-settled', 'false')

      act(() => {
        now = 450
        animationFrame.step(now)
      })
      const midLine = screen.getByTestId('throughput-chart-line').getAttribute('d')
      expect(midLine).not.toBe(firstLine)

      act(() => {
        now = 900
        animationFrame.step(now)
      })
      const finalLine = screen.getByTestId('throughput-chart-line').getAttribute('d')
      expect(finalLine).not.toBe(firstLine)
      expect(finalLine).not.toBe(midLine)
      expect(screen.getByTestId('throughput-chart')).toHaveAttribute('data-scale-settled', 'true')
    } finally {
      nowSpy.mockRestore()
      animationFrame.restore()
    }
  })

  it('renders sample changes immediately while easing the progress edge', () => {
    let now = 0
    const animationFrame = installAnimationFrameMock()
    const nowSpy = vi.spyOn(performance, 'now').mockImplementation(() => now)
    const initialSamples = [sample(20, 150), sample(40, 180), sample(60, 210)]
    const nextSamples = [sample(20, 150), sample(40, 260), sample(70, 420)]

    try {
      const { rerender } = render(
        <ThroughputChart samples={initialSamples} currentPercent={60} peakRate={1000} />,
      )
      const firstLine = screen.getByTestId('throughput-chart-line').getAttribute('d')
      expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '60')

      act(() => {
        rerender(<ThroughputChart samples={nextSamples} currentPercent={70} peakRate={1100} />)
      })

      // The reshaped trace appears at once (no multi-frame tween drags it around),
      // while the progress edge has not yet jumped — it eases from 60 toward 70.
      expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).not.toBe(firstLine)
      expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '60')

      act(() => {
        now = 480
        animationFrame.step(now)
      })
      expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '70')
      expect(screen.getByTestId('throughput-chart')).toHaveAttribute('data-scale-settled', 'true')
    } finally {
      nowSpy.mockRestore()
      animationFrame.restore()
    }
  })

  it('pegs a reading above the frozen ceiling at the top instead of rescaling', () => {
    render(
      <ThroughputChart
        samples={[sample(20, 250), sample(40, 250), sample(60, 4000)]}
        currentPercent={60}
        peakRate={1000}
      />,
    )

    // 4000 > the frozen ceiling (1000): the line clamps to the top band rather
    // than forcing the scale to grow this render.
    expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toContain(',8')
  })

  it('renders a stalled single-point chart at the baseline', () => {
    render(<ThroughputChart samples={[sample(55, 0)]} currentPercent={55} peakRate={0} />)

    expect(screen.getByTestId('throughput-chart-baseline')).toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-leading-edge').style.left).toBe('55%')
  })

  it('eases a newly committed point in from the previous point over an append', () => {
    let now = 0
    const animationFrame = installAnimationFrameMock()
    const nowSpy = vi.spyOn(performance, 'now').mockImplementation(() => now)
    const initial = [sample(10, 400), sample(20, 800), sample(30, 1200)]

    try {
      const { rerender } = render(
        <ThroughputChart samples={initial} currentPercent={30} peakRate={10000} />,
      )

      // A new committed point appends (same scale band, so only the tail moves).
      act(() => {
        rerender(
          <ThroughputChart
            samples={[...initial, sample(40, 1600)]}
            currentPercent={40}
            peakRate={10000}
          />,
        )
      })

      // The new tail starts collapsed onto the previous point — it has not popped
      // out to 40% yet — and the chart reports itself as still animating.
      const startD = screen.getByTestId('throughput-chart-line').getAttribute('d')
      expect(startD).not.toContain('40,')
      expect(screen.getByTestId('throughput-chart')).toHaveAttribute('data-scale-settled', 'false')

      // Part-way through, the tail has advanced but has not reached the endpoint.
      act(() => {
        now = 130
        animationFrame.step(now)
      })
      const midD = screen.getByTestId('throughput-chart-line').getAttribute('d')
      expect(midD).not.toBe(startD)
      expect(midD).not.toContain('40,')
      expect(screen.getByTestId('throughput-chart')).toHaveAttribute('data-scale-settled', 'false')

      // Completed: the tail lands on the committed point and the chart settles.
      act(() => {
        now = 480
        animationFrame.step(now)
      })
      expect(screen.getByTestId('throughput-chart-line').getAttribute('d')).toContain('40,')
      expect(screen.getByTestId('throughput-chart')).toHaveAttribute('data-scale-settled', 'true')
    } finally {
      nowSpy.mockRestore()
      animationFrame.restore()
    }
  })
})
