import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import type { ThroughputSample } from '@/lib/types/ipc'

function sample(percent: number, rate: number): ThroughputSample {
  return { percent, rate }
}

describe('ThroughputChart', () => {
  it('renders area, line, ceiling, progress fill extent, dot, and the scale label', () => {
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
    expect(screen.getByTestId('throughput-chart-fill-extent')).toHaveAttribute('width', '81')
    expect(screen.getByTestId('throughput-chart-progress-band')).toHaveAttribute('width', '81')
    expect(screen.getByTestId('throughput-chart-track')).toHaveAttribute('x1', '81')
    expect(screen.getByTestId('throughput-chart-dot').style.left).toBe('81%')
    // The label shows the frozen scale max (the session peak).
    expect(screen.getByTestId('throughput-chart-ceiling-label')).toHaveTextContent('260 B/s')
  })

  it('renders a baseline and dot without line or area when fewer than three samples exist', () => {
    render(
      <ThroughputChart
        samples={[sample(18, 140), sample(42, 180)]}
        currentPercent={42}
        peakRate={180}
      />,
    )

    expect(screen.getByTestId('throughput-chart-baseline')).toBeInTheDocument()
    expect(screen.queryByTestId('throughput-chart-area')).not.toBeInTheDocument()
    expect(screen.queryByTestId('throughput-chart-line')).not.toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-dot').style.left).toBe('42%')
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
    const dot = screen.getByTestId('throughput-chart-dot')
    expect(line.getAttribute('d')).toContain('77,46')
    expect(dot.style.left).toBe('77%')
    expect(dot.style.top).toBe('95.833%')
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
    expect(screen.getByTestId('throughput-chart-dot').style.left).toBe('63%')
  })

  it('scales to the frozen peak ceiling, not the visible samples, so the scale never rescales', () => {
    const steady = [sample(20, 250), sample(40, 250), sample(60, 250)]
    const { rerender } = render(
      <ThroughputChart samples={steady} currentPercent={60} peakRate={1000} />,
    )
    expect(screen.getByTestId('throughput-chart-ceiling-label')).toHaveTextContent('1000 B/s')
    expect(screen.getByTestId('throughput-chart-dot').style.top).toBe('76.984%')

    // The same steady point sits lower under a higher frozen peak: the Y scale
    // follows the (ratcheting) peak, never the current sample spread.
    rerender(<ThroughputChart samples={steady} currentPercent={60} peakRate={2000} />)
    expect(screen.getByTestId('throughput-chart-dot').style.top).toBe('86.409%')
  })

  it('pegs a reading above the frozen ceiling at the top instead of rescaling', () => {
    render(
      <ThroughputChart
        samples={[sample(20, 250), sample(40, 250), sample(60, 4000)]}
        currentPercent={60}
        peakRate={1000}
      />,
    )

    // 4000 > the frozen ceiling (1000): the dot clamps to the top band rather
    // than forcing the scale to grow this render.
    const dotTop = Number.parseFloat(screen.getByTestId('throughput-chart-dot').style.top)
    expect(dotTop).toBeLessThan(20)
  })

  it('renders a stalled single-point chart at the baseline', () => {
    render(<ThroughputChart samples={[sample(55, 0)]} currentPercent={55} peakRate={0} />)

    expect(screen.getByTestId('throughput-chart-baseline')).toBeInTheDocument()
    const dot = screen.getByTestId('throughput-chart-dot')
    expect(dot.style.left).toBe('55%')
    expect(dot.style.top).toBe('95.833%')
  })
})
