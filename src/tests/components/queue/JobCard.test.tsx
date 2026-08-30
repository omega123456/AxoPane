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
    totalBytes: 85_899_345_920,
    copiedBytes: 11_166_914_969,
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
  it('renders the header, job total, bar, current file, rate, chart and controls', () => {
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
    expect(screen.getByText('footage, b-roll')).toBeInTheDocument()
    expect(screen.getByText('D:\\dst')).toBeInTheDocument()
    expect(screen.getByText('10.4 GB of 80.0 GB · about 3 min left')).toBeInTheDocument()
    expect(screen.getByText('63%')).toBeInTheDocument()
    expect(screen.getByText('master-reel-final.mkv')).toBeInTheDocument()
    expect(screen.getByText('item 813 of 1,248')).toBeInTheDocument()
    expect(screen.getByText('248.0 MB/s')).toBeInTheDocument()
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

  it('does not print a speed anywhere except the rate line', () => {
    render(
      <JobCard
        operation={progress({})}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    // The chart used to label its Y-axis ceiling with a second, larger
    // `/s` value that read as a competing speed reading.
    expect(screen.getAllByText(/\/s$/)).toHaveLength(1)
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
    expect(screen.getByText('10.4 GB of 80.0 GB · estimating…')).toBeInTheDocument()
  })

  it('says the total is still growing while the backend keeps discovering bytes', () => {
    vi.useFakeTimers()
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
    try {
      expect(screen.getByText('10.4 GB of 80.0 GB · about 3 min left')).toBeInTheDocument()

      // Rust grows `total_bytes` as it walks the selected folders. A total that
      // is still climbing must not be presented as final.
      act(() => {
        rerender(
          <JobCard
            operation={progress({ totalBytes: 128_849_018_880 })}
            throughputHistory={samples([63, 260_046_848])}
            throughputPeak={260_046_848}
            hasConflict={false}
            reorderable={false}
            {...noopHandlers()}
          />,
        )
      })
      act(() => {
        vi.advanceTimersByTime(500)
      })
      expect(screen.getByText('10.4 GB of 120.0 GB (still scanning)')).toBeInTheDocument()

      // Once the scan settles the total stops moving and the estimate returns.
      act(() => {
        vi.advanceTimersByTime(500)
      })
      expect(screen.getByText('10.4 GB of 120.0 GB · about 3 min left')).toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
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
    const bar = screen.getByRole('progressbar', { name: 'Copying 1,248 items' })

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
    expect(bar).toBeInTheDocument()

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

  it('renders a Resume control when paused, and neither an estimate nor a stale rate', async () => {
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
    expect(screen.getByText('10.4 GB of 80.0 GB · paused')).toBeInTheDocument()
    expect(screen.getByText('—')).toBeInTheDocument()
    const resume = screen.getByRole('button', { name: /Resume/ })
    const skip = screen.getByRole('button', { name: /Skip/ })
    expect(skip).toBeEnabled()
    await user.click(skip)
    expect(handlers.onSkip).toHaveBeenCalled()
    await user.click(resume)
    expect(handlers.onResume).toHaveBeenCalled()
  })

  it('renders completed state with a dismiss control and no progress bar', async () => {
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

  it('renders failed state with a retry control, error message and no progress bar', async () => {
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

  it('renders cancelled state with a dismiss control, retained note box and no progress bar', async () => {
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

  it('shows what is being copied and where, on one line', () => {
    render(
      <JobCard
        operation={progress({
          itemNames: ['Season 01', 'poster.jpg'],
          destinationDir: 'D:\\Sorted',
          totalItems: 2,
        })}
        throughputHistory={samples([22, 240_000_000])}
        throughputPeak={240_000_000}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    expect(screen.getByText('Season 01, poster.jpg')).toBeInTheDocument()
    expect(screen.getByText('D:\\Sorted')).toHaveAttribute('title', 'D:\\Sorted')
    // Nothing is hidden, so there is no disclosure to offer.
    expect(screen.queryByRole('button', { name: /more/ })).not.toBeInTheDocument()
  })

  it('expands the full selection when more items are queued than the line shows', async () => {
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({
          itemNames: ['Season 01', 'Season 02', 'Season 03', 'Season 04', 'Season 05'],
          totalItems: 5,
          completedItems: 1,
        })}
        throughputHistory={samples([22, 240_000_000])}
        throughputPeak={240_000_000}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    expect(screen.getByText('Season 01, Season 02')).toBeInTheDocument()
    expect(screen.queryByText('Season 03')).not.toBeInTheDocument()

    const toggle = screen.getByRole('button', { name: /\+3 more/ })
    expect(toggle).toHaveAttribute('aria-expanded', 'false')
    await user.click(toggle)

    expect(toggle).toHaveAttribute('aria-expanded', 'true')
    const list = screen.getByRole('list')
    expect(within(list).getAllByRole('listitem')).toHaveLength(5)
    expect(within(list).getByText('Season 03')).toBeInTheDocument()

    await user.click(toggle)
    expect(screen.queryByRole('list')).not.toBeInTheDocument()
  })

  it('says how many names the backend did not send for an oversized selection', async () => {
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ itemNames: ['footage', 'b-roll'] })}
        throughputHistory={samples([22, 240_000_000])}
        throughputPeak={240_000_000}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )

    await user.click(screen.getByRole('button', { name: /\+1,246 more/ }))
    const list = screen.getByRole('list')
    expect(within(list).getByText('+1,246 more')).toBeInTheDocument()
  })

  it('exposes the destination and current file name via title tooltips when truncated', () => {
    render(
      <JobCard
        operation={progress({
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

    expect(screen.getByText('F:\\Download')).toHaveAttribute('title', 'F:\\Download')
    expect(screen.getByText('server.mjs')).toHaveAttribute('title', 'server.mjs')
  })

  it('shows a resolve action while in conflict and keeps the progress bar', async () => {
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

  it('counts items rather than bytes for a delete, and shows no destination', () => {
    render(
      <JobCard
        operation={progress({
          kind: 'delete',
          destinationDir: '',
          itemNames: ['footage', 'b-roll'],
          totalItems: 500,
          completedItems: 412,
          etaSeconds: 8,
        })}
        throughputHistory={samples([63, 260_046_848])}
        throughputPeak={260_046_848}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Deleting 500 items')).toBeInTheDocument()
    expect(screen.getByText('footage, b-roll')).toBeInTheDocument()
    expect(screen.getByText('412 of 500 items deleted · about 8 sec left')).toBeInTheDocument()
    expect(screen.getByText('item 413 of 500')).toBeInTheDocument()
    expect(screen.queryByText('D:\\dst')).not.toBeInTheDocument()
    // Items, not bytes: a transfer rate and a throughput curve would report
    // something the rest of the card does not.
    expect(screen.queryByText(/\/s$/)).not.toBeInTheDocument()
    expect(screen.queryByTestId('throughput-chart-line')).not.toBeInTheDocument()
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

  it('announces the job total and time left, throttled to five seconds', () => {
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
      expect(liveRegion).toHaveTextContent('10.4 GB of 80.0 GB, about 3 min left')

      act(() => {
        rerender(
          <JobCard
            operation={progress({ copiedBytes: 22_888_226_816, etaSeconds: 120 })}
            throughputHistory={samples([63, 300_000_000])}
            throughputPeak={300_000_000}
            hasConflict={false}
            reorderable={false}
            {...noopHandlers()}
          />,
        )
      })

      // The visible numbers are throttled too, so right after the rerender the
      // card still shows the previous values.
      expect(screen.getByText('10.4 GB of 80.0 GB · about 3 min left')).toBeInTheDocument()
      expect(liveRegion).toHaveTextContent('10.4 GB of 80.0 GB, about 3 min left')

      // After the metrics refresh interval the visible card catches up…
      act(() => {
        vi.advanceTimersByTime(1000)
      })
      expect(screen.getByText('21.3 GB of 80.0 GB · about 2 min left')).toBeInTheDocument()
      expect(screen.getByText('286.1 MB/s')).toBeInTheDocument()
      // …but the slower 5s live region is still on the old announcement.
      expect(liveRegion).toHaveTextContent('10.4 GB of 80.0 GB, about 3 min left')

      // The live region announces what the card shows, so its 5s throttle only
      // starts once the visible numbers change.
      act(() => {
        vi.advanceTimersByTime(4999)
      })
      expect(liveRegion).toHaveTextContent('10.4 GB of 80.0 GB, about 3 min left')

      act(() => {
        vi.advanceTimersByTime(1)
      })
      expect(liveRegion).toHaveTextContent('21.3 GB of 80.0 GB, about 2 min left')
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
      expect(liveRegion).toHaveTextContent('10.4 GB of 80.0 GB, about 3 min left')

      act(() => {
        rerender(
          <JobCard
            operation={progress({ copiedBytes: 22_888_226_816, etaSeconds: 120 })}
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
      expect(liveRegion).toHaveTextContent('10.4 GB of 80.0 GB, about 3 min left')
    } finally {
      vi.useRealTimers()
    }
  })
})
