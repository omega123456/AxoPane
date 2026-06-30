import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LogViewer } from '@/components/dialogs/LogViewer'
import { ipc } from '@/tests/ipc-mock'
import type { LogEntry } from '@/lib/types/ipc'

function entry(id: number, level: string, message: string): LogEntry {
  return {
    id,
    level,
    target: 'file_explorer_lib::test',
    message,
    timestamp: '2026-06-30T09:15:04Z',
  }
}

describe('LogViewer', () => {
  beforeEach(() => {
    ipc.install()
  })

  afterEach(() => {
    ipc.reset()
  })

  it('renders entries newest-first from the current day file', async () => {
    ipc.override('read_logs', [
      entry(0, 'info', 'oldest line'),
      entry(1, 'error', 'newest line'),
    ])

    render(<LogViewer />)

    await screen.findByText('newest line')
    const rows = screen.getAllByRole('row')
    // Header row + 2 data rows.
    expect(rows).toHaveLength(3)
    expect(within(rows[1]).getByText('newest line')).toBeInTheDocument()
    expect(within(rows[2]).getByText('oldest line')).toBeInTheDocument()
  })

  it('formats the timestamp in UTC', async () => {
    ipc.override('read_logs', [entry(0, 'info', 'a line')])
    render(<LogViewer />)
    expect(await screen.findByText('Jun 30 09:15:04')).toBeInTheDocument()
  })

  it('filters by severity floor', async () => {
    ipc.override('read_logs', [
      entry(0, 'debug', 'debug line'),
      entry(1, 'info', 'info line'),
      entry(2, 'error', 'error line'),
    ])
    const user = userEvent.setup()
    render(<LogViewer />)
    await screen.findByText('debug line')

    await user.selectOptions(screen.getByLabelText('Severity filter'), 'warn')

    expect(screen.queryByText('debug line')).not.toBeInTheDocument()
    expect(screen.queryByText('info line')).not.toBeInTheDocument()
    expect(screen.getByText('error line')).toBeInTheDocument()
  })

  it('paginates 20 entries per page', async () => {
    const many = Array.from({ length: 25 }, (_, index) =>
      entry(index, 'info', `line ${index}`),
    )
    ipc.override('read_logs', many)
    const user = userEvent.setup()
    render(<LogViewer />)

    await screen.findByText('1-20 of 25')
    expect(screen.getByText('Page 1 / 2')).toBeInTheDocument()

    await user.click(screen.getByLabelText('Next page'))
    expect(screen.getByText('21-25 of 25')).toBeInTheDocument()
    expect(screen.getByText('Page 2 / 2')).toBeInTheDocument()
  })

  it('refetches when the refresh button is clicked', async () => {
    let calls = 0
    ipc.override('read_logs', () => {
      calls += 1
      return [entry(0, 'info', `load ${calls}`)]
    })
    const user = userEvent.setup()
    render(<LogViewer />)
    await screen.findByText('load 1')

    await user.click(screen.getByTestId('log-viewer-refresh'))
    await waitFor(() => expect(screen.getByText('load 2')).toBeInTheDocument())
  })

  it('auto-refreshes on an interval and can be toggled off', async () => {
    vi.useFakeTimers()
    try {
      let calls = 0
      ipc.override('read_logs', () => {
        calls += 1
        return [entry(0, 'info', `tick ${calls}`)]
      })
      render(<LogViewer />)
      // Flush the IPC promise chain (.then then .finally) inside act so state
      // updates from the initial load don't escape and trigger act() warnings.
      await act(async () => {
        await Promise.resolve() // .then: setEntries + setHasError
        await Promise.resolve() // .finally: setIsLoading + setIsRefreshing
      })
      expect(screen.getByText('tick 1')).toBeInTheDocument()

      await act(async () => {
        await vi.advanceTimersByTimeAsync(5000)
        // advanceTimersByTimeAsync flushes one microtask level internally (.then);
        // flush one more to capture the .finally state updates.
        await Promise.resolve()
      })
      expect(screen.getByText('tick 2')).toBeInTheDocument()

      const toggle = screen.getByTestId('log-viewer-auto-refresh')
      act(() => {
        toggle.click()
      })
      expect(toggle).toHaveAttribute('aria-pressed', 'false')

      await act(async () => {
        await vi.advanceTimersByTimeAsync(5000)
      })
      // No further fetches after disabling auto-refresh.
      expect(screen.getByText('tick 2')).toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })

  it('shows an empty state when there are no entries', async () => {
    ipc.override('read_logs', [])
    render(<LogViewer />)
    expect(await screen.findByTestId('log-viewer-empty')).toBeInTheDocument()
  })

  it('shows an error state when the read fails', async () => {
    ipc.override('read_logs', () => {
      throw new Error('disk gone')
    })
    render(<LogViewer />)
    expect(await screen.findByTestId('log-viewer-error')).toBeInTheDocument()
  })
})
