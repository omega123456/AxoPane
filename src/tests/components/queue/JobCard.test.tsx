import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { JobCard } from '@/components/queue/JobCard'
import type { OpProgress } from '@/lib/types/ipc'

function progress(overrides: Partial<OpProgress>): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'C:\\src',
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
    onSkip: vi.fn(),
    onRetry: vi.fn(),
    onResolve: vi.fn(),
  }
}

describe('JobCard', () => {
  it('renders the active copy header, percent, current file and controls', () => {
    render(
      <JobCard operation={progress({})} hasConflict={false} reorderable={false} {...noopHandlers()} />,
    )
    expect(screen.getByText('Copying 1,248 items')).toBeInTheDocument()
    expect(screen.getByText('63%')).toBeInTheDocument()
    expect(screen.getByText('master-reel-final.mkv')).toBeInTheDocument()
    expect(screen.getByText('812 / 1,248 items')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Pause/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Skip/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Cancel/ })).toBeInTheDocument()
  })

  it('shows estimating until the ETA stabilizes', () => {
    render(
      <JobCard
        operation={progress({ etaSeconds: null })}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('estimating…')).toBeInTheDocument()
  })

  it('renders a Resume control when paused', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'paused' })}
        hasConflict={false}
        reorderable={false}
        {...handlers}
      />,
    )
    const resume = screen.getByRole('button', { name: /Resume/ })
    await user.click(resume)
    expect(handlers.onResume).toHaveBeenCalled()
  })

  it('renders completed state with no controls', () => {
    render(
      <JobCard
        operation={progress({ status: 'completed', progressPercent: 100, currentFileName: null })}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Copying complete')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Pause/ })).not.toBeInTheDocument()
  })

  it('renders failed state with a retry control and error message', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'failed', errorMessage: 'disk full' })}
        hasConflict={false}
        reorderable={false}
        {...handlers}
      />,
    )
    expect(screen.getByText('Copying failed')).toBeInTheDocument()
    expect(screen.getByText('disk full')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Retry/ }))
    expect(handlers.onRetry).toHaveBeenCalled()
  })

  it('exposes reorder controls when reorderable', async () => {
    const onMoveUp = vi.fn()
    const onMoveDown = vi.fn()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'pending' })}
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

  it('shows a resolve action while in conflict', async () => {
    const handlers = noopHandlers()
    const user = userEvent.setup()
    render(
      <JobCard
        operation={progress({ status: 'conflict' })}
        hasConflict
        reorderable={false}
        {...handlers}
      />,
    )
    await user.click(screen.getByRole('button', { name: /Resolve conflict/ }))
    expect(handlers.onResolve).toHaveBeenCalled()
  })

  it('labels a move operation', () => {
    render(
      <JobCard
        operation={progress({ kind: 'move' })}
        hasConflict={false}
        reorderable={false}
        {...noopHandlers()}
      />,
    )
    expect(screen.getByText('Moving 1,248 items')).toBeInTheDocument()
  })
})
