import { act, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { JobCard } from '@/components/queue/JobCard'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'

function progress(overrides: Partial<OpProgress>): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'C:\\src',
    itemNames: ['footage'],
    destinationDir: 'D:\\dst',
    totalItems: 1248,
    completedItems: 812,
    totalBytes: 1000,
    copiedBytes: 630,
    progressPercent: 63,
    bytesPerSecond: 260_046_848,
    etaSeconds: 180,
    currentFileName: 'master-reel-final.mkv',
    currentFileCopiedBytes: 600,
    currentFileTotalBytes: 1000,
    errorMessage: null,
    ...overrides,
  }
}

function noopHandlers() {
  return {
    onPause: vi.fn(),
    onResume: vi.fn(),
    onCancel: vi.fn(),
    onDismiss: vi.fn(),
    onSkip: vi.fn(),
    onRetry: vi.fn(),
    onResolve: vi.fn(),
  }
}

function samples(...entries: Array<[number, number]>): ThroughputSample[] {
  return entries.map(([percent, rate]) => ({ percent, rate }))
}

describe('JobCard', () => {
  it('renders the active copy header, percent, current file, chart and controls', () => {
    render(
      <JobCard
        operation={progress({ itemNames: ['footage', 'b-roll'] })}
        throughputHistory={samples([22, 240_000_000], [41, 250_000_000], [63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Copying 1,248 items')).toBeInTheDocument()
    expect(screen.getByText('63%')).toBeInTheDocument()
    expect(screen.getByText('C:\\src\\footage, b-roll, +1,246 more')).toBeInTheDocument()
    expect(screen.getByText('master-reel-final.mkv')).toBeInTheDocument()
    expect(screen.getByText('813 / 1,248 items')).toBeInTheDocument()
    expect(screen.getByTestId('throughput-chart-line')).toBeInTheDocument()
    expect(screen.getByRole('progressbar', { name: 'Copying 1,248 items' })).toHaveAttribute(
      'aria-valuenow',
      '63',
    )
    expect(screen.getByTestId('throughput-chart-progress-fill')).toHaveAttribute('width', '63')
    expect(screen.getByRole('button', { name: /Pause/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Skip/ })).toBeDisabled()
    expect(screen.getByRole('button', { name: /Cancel/ })).toBeInTheDocument()
  })

  it('shows estimating until the ETA stabilizes', () => {
    render(
      <JobCard
        operation={progress({ etaSeconds: null })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('estimating…')).toBeInTheDocument()
  })

  it('keeps showing the last current file while the backend briefly clears it between files', () => {
    const { rerender } = render(
      <JobCard
        operation={progress({})}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('master-reel-final.mkv')).toBeInTheDocument()
    const chartProgressbar = screen.getByRole('progressbar', {
      name: 'Copying 1,248 items',
    })

    // The backend clears currentFileName for an instant between finishing one
    // file and starting the next; the block must stay mounted and keep
    // showing the last file instead of disappearing and reappearing.
    rerender(
      <JobCard
        operation={progress({
          currentFileName: null,
          currentFileCopiedBytes: 0,
          currentFileTotalBytes: 0,
        })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('master-reel-final.mkv')).toBeInTheDocument()
    expect(chartProgressbar).toBeInTheDocument()

    rerender(
      <JobCard
        operation={progress({ currentFileName: 'next-clip.mkv' })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('next-clip.mkv')).toBeInTheDocument()
  })

  it('renders a Resume control when paused and keeps the chart surface stable', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'paused' })}
        throughputHistory={samples([58, 260_046_848], [63, 0])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...handlers}
      />,
    )
    expect(screen.getByRole('progressbar', { name: 'Copying 1,248 items' })).toHaveAttribute(
      'aria-valuenow',
      '63',
    )
    const resume = screen.getByRole('button', { name: /Resume/ })
    const skip = screen.getByRole('button', { name: /Skip/ })
    expect(skip).toBeEnabled()
    await user.click(skip)
    expect(handlers.onSkip).toHaveBeenCalled()
    await user.click(resume)
    expect(handlers.onResume).toHaveBeenCalled()
  })

  it('renders completed state with a dismiss control and no chart progressbar', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'completed', progressPercent: 100, currentFileName: null })}
        throughputHistory={samples([63, 260_046_848], [82, 200_000_000], [100, 0])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...handlers}
      />,
    )
    expect(screen.getByText('Copying complete')).toBeInTheDocument()
    expect(screen.queryByRole('progressbar', { name: 'Copying complete' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Pause/ })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Dismiss/ }))
    expect(handlers.onDismiss).toHaveBeenCalled()
  })

  it('renders failed state with a retry control, error message and no chart progressbar', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'failed', errorMessage: 'disk full' })}
        throughputHistory={samples([63, 260_046_848], [72, 120_000_000], [72, 0])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...handlers}
      />,
    )
    expect(screen.getByText('Copying failed')).toBeInTheDocument()
    expect(screen.getByText('disk full')).toBeInTheDocument()
    expect(screen.queryByRole('progressbar', { name: 'Copying failed' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Retry/ }))
    expect(handlers.onRetry).toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: /Dismiss/ }))
    expect(handlers.onDismiss).toHaveBeenCalled()
  })

  it('renders cancelled state with a dismiss control, retained note box and no chart progressbar', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'cancelled', currentFileName: null })}
        throughputHistory={samples([63, 260_046_848], [67, 190_000_000], [67, 0])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...handlers}
      />,
    )
    expect(screen.getByText('Copying cancelled')).toBeInTheDocument()
    expect(screen.getByText(/Any completed file changes were kept/i)).toBeInTheDocument()
    expect(screen.queryByRole('progressbar', { name: 'Copying cancelled' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Pause/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Cancel/ })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Dismiss/ }))
    expect(handlers.onDismiss).toHaveBeenCalled()
  })

  it('exposes reorder controls when reorderable', async () => {
    const onMoveUp = vi.fn()
    const onMoveDown = vi.fn()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'pending' })}
        throughputHistory={samples([0, 0])}
        throughputPeak={0}
        hasConflict={false}
        reorderable
        {...noopHandlers()}
        onMoveUp={onMoveUp}
        onMoveDown={onMoveDown}
      />,
    )
    await user.click(screen.getByRole('button', { name: 'Move job up' }))
    await user.click(screen.getByRole('button', { name: 'Move job down' }))
    expect(onMoveUp).toHaveBeenCalled()
    expect(onMoveDown).toHaveBeenCalled()
  })

  it('appends the queued item names to both the source and destination paths', () => {
    render(
      <JobCard
        operation={progress({
          status: 'pending',
          sourceDir: 'C:\\Downloads',
          itemNames: ['Season 01', 'poster.jpg', 'notes.txt'],
          destinationDir: 'D:\\Sorted',
          totalItems: 3,
        })}
        throughputHistory={samples([0, 0])}
        throughputPeak={0}
        hasConflict={false}
        reorderable
        {...noopHandlers()}
      />,
    )

    expect(screen.getByText('C:\\Downloads\\Season 01, poster.jpg, +1 more')).toBeInTheDocument()
    expect(screen.getByText('D:\\Sorted\\Season 01, poster.jpg, +1 more')).toBeInTheDocument()
  })

  it('appends the top-level item name to both paths while the job is actively running', () => {
    render(
      <JobCard
        operation={progress({
          status: 'active',
          sourceDir: 'D:\\projects',
          itemNames: ['b'],
          totalItems: 1,
          destinationDir: 'F:\\Download',
          currentFileName: 'server.mjs',
        })}
        throughputHistory={samples([22, 240_000_000])}
        throughputPeak={240_000_000}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    expect(screen.getByText('D:\\projects\\b')).toBeInTheDocument()
    expect(screen.getByText('F:\\Download\\b')).toBeInTheDocument()
    expect(screen.getByText('server.mjs')).toBeInTheDocument()
  })

  it('exposes the full path and current file name via title tooltips when truncated', () => {
    render(
      <JobCard
        operation={progress({
          status: 'active',
          sourceDir: 'D:\\projects',
          itemNames: ['b'],
          totalItems: 1,
          destinationDir: 'F:\\Download',
          currentFileName: 'server.mjs',
        })}
        throughputHistory={samples([22, 240_000_000])}
        throughputPeak={240_000_000}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    expect(screen.getByText('D:\\projects\\b')).toHaveAttribute('title', 'D:\\projects\\b')
    expect(screen.getByText('F:\\Download\\b')).toHaveAttribute('title', 'F:\\Download\\b')
    expect(screen.getByText('server.mjs')).toHaveAttribute('title', 'server.mjs')
  })

  it('shows a resolve action while in conflict and keeps the chart progressbar', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'conflict' })}
        throughputHistory={samples([33, 260_046_848], [63, 0])}
        throughputPeak={260_046_848}
        hasConflict
        reorderable={false}
        {...handlers}
      />,
    )
    expect(screen.getByRole('progressbar', { name: 'Copying 1,248 items' })).toHaveAttribute(
      'aria-valuenow',
      '63',
    )
    await user.click(screen.getByRole('button', { name: /Resolve conflict/ }))
    expect(handlers.onResolve).toHaveBeenCalled()
  })

  it('labels a move operation', () => {
    render(
      <JobCard
        operation={progress({ kind: 'move' })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Moving 1,248 items')).toBeInTheDocument()
  })

  it('labels a delete operation', () => {
    render(
      <JobCard
        operation={progress({
          kind: 'delete',
          destinationDir: '',
          itemNames: ['footage', 'b-roll'],
        })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Deleting 1,248 items')).toBeInTheDocument()
    expect(screen.getByText('C:\\src\\footage, b-roll, +1,246 more')).toBeInTheDocument()
    expect(screen.queryByText('D:\\dst')).not.toBeInTheDocument()
  })

  it('labels archive operations', () => {
    const { rerender } = render(
      <JobCard
        operation={progress({ kind: 'compress' })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Compressing 1,248 items')).toBeInTheDocument()

    rerender(
      <JobCard
        operation={progress({ kind: 'extract' })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Extracting 1,248 items')).toBeInTheDocument()
  })

  it('throttles live metrics announcements for five seconds on a dedicated hidden node', () => {
    vi.useFakeTimers()
    const { rerender, container } = render(
      <JobCard
        operation={progress({})}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    const liveRegion = container.querySelector('[aria-live="polite"]')
    try {
      expect(liveRegion).toHaveClass('sr-only')
      expect(liveRegion?.parentElement).not.toHaveAttribute('aria-live')
      expect(liveRegion).toHaveTextContent('248.0 MB/s, about 3 min left, 813 / 1,248 items')

      act(() => {
        rerender(
          <JobCard
            operation={progress({
              bytesPerSecond: 300_000_000,
              etaSeconds: 120,
              completedItems: 900,
            })}
            throughputHistory={samples([63, 300_000_000])}
            throughputPeak={300_000_000}
            hasConflict={false}
            reorderable={false}
            {...noopHandlers()}
          />,
        )
      })

      // The visible metrics row is throttled too, so right after the rerender it
      // still shows the previous values. (Scope to the metrics row — the chart's
      // ceiling label can carry the same rate string when speed equals the peak.)
      const metricsRow = liveRegion?.parentElement as HTMLElement
      expect(within(metricsRow).getByText('248.0 MB/s')).toBeInTheDocument()
      expect(liveRegion).toHaveTextContent('248.0 MB/s, about 3 min left, 813 / 1,248 items')

      // After the metrics refresh interval the visible row catches up…
      act(() => {
        vi.advanceTimersByTime(1000)
      })
      expect(within(metricsRow).getByText('286.1 MB/s')).toBeInTheDocument()
      expect(screen.getByText('about 2 min left')).toBeInTheDocument()
      expect(screen.getByText('901 / 1,248 items')).toBeInTheDocument()
      // …but the slower 5s live region is still on the old announcement.
      expect(liveRegion).toHaveTextContent('248.0 MB/s, about 3 min left, 813 / 1,248 items')

      act(() => {
        vi.advanceTimersByTime(3999)
      })
      expect(liveRegion).toHaveTextContent('248.0 MB/s, about 3 min left, 813 / 1,248 items')

      act(() => {
        vi.advanceTimersByTime(1)
      })
      expect(liveRegion).toHaveTextContent('286.1 MB/s, about 2 min left, 901 / 1,248 items')
    } finally {
      vi.useRealTimers()
    }
  })

  it('drops a pending announcement when metrics return to the current announced value before the throttle fires', () => {
    vi.useFakeTimers()
    const { rerender, container } = render(
      <JobCard
        operation={progress({})}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    const liveRegion = container.querySelector('[aria-live="polite"]')
    try {
      expect(liveRegion).toHaveTextContent('248.0 MB/s, about 3 min left, 813 / 1,248 items')

      act(() => {
        rerender(
          <JobCard
            operation={progress({
              bytesPerSecond: 300_000_000,
              etaSeconds: 120,
              completedItems: 900,
            })}
            throughputHistory={samples([63, 300_000_000])}
            throughputPeak={300_000_000}
            hasConflict={false}
            reorderable={false}
            {...noopHandlers()}
          />,
        )
      })

      act(() => {
        rerender(
          <JobCard
            operation={progress({})}
            throughputHistory={samples([63, 260_046_848])}
            throughputPeak={260_046_848}
            hasConflict={false}
            reorderable={false}
            {...noopHandlers()}
          />,
        )
      })

      act(() => {
        vi.advanceTimersByTime(5000)
      })
      expect(liveRegion).toHaveTextContent('248.0 MB/s, about 3 min left, 813 / 1,248 items')
    } finally {
      vi.useRealTimers()
    }
  })
})
