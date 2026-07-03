import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { QueueOverlay } from '@/components/queue/QueueOverlay'
import type { OpProgress, OpSnapshot } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { useQueueStore } from '@/stores/queue-store'

function progress(overrides: Partial<OpProgress>): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'C:\\src',
    itemNames: ['a.txt'],
    destinationDir: 'D:\\dst',
    totalItems: 4,
    completedItems: 1,
    totalBytes: 1000,
    copiedBytes: 250,
    progressPercent: 25,
    bytesPerSecond: 100,
    etaSeconds: 12,
    currentFileName: 'a.txt',
    currentFileCopiedBytes: 50,
    currentFileTotalBytes: 100,
    errorMessage: null,
    ...overrides,
  }
}

  beforeEach(() => {
    ipc.install()
    useConfigStore.getState().reset()
    useQueueStore.getState().reset()
  })

describe('QueueOverlay', () => {
  it('renders nothing when there is no queue work', async () => {
    const { container } = render(<QueueOverlay />)
    await waitFor(() => {
      expect(useQueueStore.getState().order).toEqual([])
      expect(container).toBeEmptyDOMElement()
    })
  })

  it('hydrates from the queue snapshot on mount', async () => {
    const snapshot: OpSnapshot[] = [{ progress: progress({}), conflict: null }]
    ipc.override('queue_snapshot', snapshot)

    render(<QueueOverlay />)
    expect(await screen.findByRole('button', { name: 'Expand job queue' })).toBeInTheDocument()
  })

  it('summarizes active delete operations as deleting', async () => {
    ipc.override('queue_snapshot', [
      { progress: progress({ kind: 'delete', destinationDir: '' }), conflict: null },
    ])

    render(<QueueOverlay />)

    expect(await screen.findByText('Deleting 1 job')).toBeInTheDocument()
  })

  it('summarizes active archive operations by their job kind', async () => {
    ipc.override('queue_snapshot', [
      { progress: progress({ kind: 'extract', itemNames: ['Archive.zip'] }), conflict: null },
    ])

    render(<QueueOverlay />)

    expect(await screen.findByText('Extracting 1 job')).toBeInTheDocument()
  })

  it('expands the collapsed toast into the panel and collapses again', async () => {
    const user = userEvent.setup()
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    render(<QueueOverlay />)

    const toast = await screen.findByRole('button', { name: 'Expand job queue' })
    await user.click(toast)

    expect(await screen.findByRole('region', { name: 'Job queue' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Fewer details/ }))
    expect(screen.queryByRole('region', { name: 'Job queue' })).not.toBeInTheDocument()
  })

  it('expands the collapsed toast when clicking the chevron area, not just the label', async () => {
    const user = userEvent.setup()
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    const { container } = render(<QueueOverlay />)

    const toast = await screen.findByRole('button', { name: 'Expand job queue' })
    const chevron = container.querySelector('.lucide-chevron-up')
    expect(chevron).not.toBeNull()
    expect(toast).toContainElement(chevron as HTMLElement)

    await user.click(chevron as Element)

    expect(await screen.findByRole('region', { name: 'Job queue' })).toBeInTheDocument()
  })

  it('caps the expanded panel height instead of reserving fixed space once jobs finish', async () => {
    ipc.override('queue_snapshot', [
      { progress: progress({ status: 'completed', currentFileName: null }), conflict: null },
    ])
    render(<QueueOverlay />)

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    const region = await screen.findByRole('region', { name: 'Job queue' })

    // A fixed height here would leave a large empty gap once the chart and
    // other active-only sections stop rendering; max-height lets the panel
    // shrink to its (now shorter) content instead.
    expect(region.className).toContain('max-h-queue-list')
    expect(region.className).not.toMatch(/(?<!max-)h-queue-list/)
  })

  it('auto-expands when a new active queue job appears and the preference is enabled', async () => {
    act(() => {
      useConfigStore.setState({ autoExpandActiveQueueToasts: true })
    })
    ipc.override('queue_snapshot', [])
    render(<QueueOverlay />)

    await waitFor(() => {
      expect(screen.queryByRole('region', { name: 'Job queue' })).not.toBeInTheDocument()
    })

    act(() => {
      ipc.emit('queue://progress', progress({}))
    })

    expect(await screen.findByRole('region', { name: 'Job queue' })).toBeInTheDocument()
  })

  it('does not auto-reopen a queue the user manually collapsed while work remains active', async () => {
    const user = userEvent.setup()
    act(() => {
      useConfigStore.setState({ autoExpandActiveQueueToasts: true })
    })
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    render(<QueueOverlay />)

    expect(await screen.findByRole('region', { name: 'Job queue' })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /Fewer details/ }))
    expect(screen.queryByRole('region', { name: 'Job queue' })).not.toBeInTheDocument()

    act(() => {
      ipc.emit('queue://progress', progress({ progressPercent: 50, copiedBytes: 500 }))
    })

    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-1'].progressPercent).toBe(50)
    })
    expect(screen.queryByRole('region', { name: 'Job queue' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Expand job queue' })).toBeInTheDocument()
  })

  it('applies live progress events', async () => {
    ipc.override('queue_snapshot', [
      { progress: progress({ progressPercent: 25 }), conflict: null },
    ])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      ipc.emit('queue://progress', progress({ progressPercent: 75, copiedBytes: 750 }))
    })

    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-1'].progressPercent).toBe(75)
    })
  })

  it('prunes a transfer card when the backend emits a removed event', async () => {
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      ipc.emit('queue://removed', 'op-1')
    })

    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-1']).toBeUndefined()
    })
    expect(useQueueStore.getState().order).toEqual([])
  })

  it('lets the collapsed toast dismiss terminal jobs', async () => {
    const user = userEvent.setup()
    ipc.override('queue_snapshot', [
      {
        progress: progress({ status: 'cancelled', currentFileName: null, bytesPerSecond: 0 }),
        conflict: null,
      },
    ])
    render(<QueueOverlay />)

    await screen.findByRole('button', { name: 'Expand job queue' })
    await user.click(screen.getByRole('button', { name: 'Dismiss job queue' }))

    await waitFor(() => {
      expect(useQueueStore.getState().order).toEqual([])
    })
    expect(screen.queryByRole('button', { name: 'Expand job queue' })).not.toBeInTheDocument()
  })

  it('Escape collapses the expanded panel rather than cancelling', async () => {
    const user = userEvent.setup()
    const cancelSpy = vi.fn(() => undefined)
    ipc.override('cancel_op', cancelSpy)
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Job queue' })

    await user.keyboard('{Escape}')
    await waitFor(() => {
      expect(useQueueStore.getState().expanded).toBe(false)
    })
    expect(cancelSpy).not.toHaveBeenCalled()
  })

  it('Space pauses and resumes the head operation while expanded', async () => {
    const user = userEvent.setup()
    ipc.override('pause_op', () => undefined)
    ipc.override('resume_op', () => undefined)
    ipc.override('queue_snapshot', [{ progress: progress({ status: 'active' }), conflict: null }])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Job queue' })

    await user.keyboard(' ')
    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-1'].status).toBe('paused')
    })
    await user.keyboard(' ')
    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-1'].status).toBe('active')
    })
  })

  it('Delete cancels and R retries the head operation while expanded', async () => {
    const user = userEvent.setup()
    const cancelSpy = vi.fn(() => undefined)
    ipc.override('cancel_op', cancelSpy)
    ipc.override('retry_op', () => undefined)
    ipc.override('queue_snapshot', [
      { progress: progress({ status: 'failed', errorMessage: 'boom' }), conflict: null },
    ])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Job queue' })

    await user.keyboard('{Delete}')
    expect(cancelSpy).toHaveBeenCalled()

    await user.keyboard('r')
    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-1'].status).toBe('pending')
    })
  })

  it('reorders pending operations via the panel controls', async () => {
    const user = userEvent.setup()
    const reorderSpy = vi.fn(() => undefined)
    ipc.override('reorder_ops', reorderSpy)
    ipc.override('queue_snapshot', [
      { progress: progress({ operationId: 'op-1', status: 'pending' }), conflict: null },
      { progress: progress({ operationId: 'op-2', status: 'pending' }), conflict: null },
    ])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Job queue' })

    const downButtons = screen.getAllByRole('button', { name: 'Move job down' })
    await user.click(downButtons[0])
    expect(reorderSpy).toHaveBeenCalled()
    await waitFor(() => {
      expect(useQueueStore.getState().order).toEqual(['op-2', 'op-1'])
    })
  })

  it('keeps the panel responsive after cancelling a queued operation', async () => {
    const user = userEvent.setup()
    const cancelSpy = vi.fn(() => undefined)
    ipc.override('cancel_op', cancelSpy)
    ipc.override('queue_snapshot', [
      { progress: progress({ operationId: 'op-1', status: 'active' }), conflict: null },
      {
        progress: progress({
          operationId: 'op-2',
          status: 'pending',
          itemNames: ['queued.zip'],
          currentFileName: null,
          bytesPerSecond: 0,
        }),
        conflict: null,
      },
    ])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand job queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Job queue' })

    await user.click(screen.getAllByRole('button', { name: /Cancel/ })[1])
    expect(cancelSpy).toHaveBeenCalledWith({ id: 'op-2' })

    act(() => {
      ipc.emit(
        'queue://progress',
        progress({
          operationId: 'op-2',
          status: 'cancelled',
          itemNames: ['queued.zip'],
          currentFileName: null,
          bytesPerSecond: 0,
        }),
      )
    })

    await waitFor(() => {
      expect(useQueueStore.getState().operations['op-2'].status).toBe('cancelled')
    })
    await user.click(screen.getByRole('button', { name: /Fewer details/ }))
    await waitFor(() => {
      expect(useQueueStore.getState().expanded).toBe(false)
    })
  })
})
