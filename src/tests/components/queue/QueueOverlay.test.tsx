import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { FilePane } from '@/components/pane/FilePane'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { QueueOverlay } from '@/components/queue/QueueOverlay'
import type { OpProgress, OpSnapshot } from '@/lib/types/ipc'
import { useQueueStore } from '@/stores/queue-store'

function progress(overrides: Partial<OpProgress>): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'C:\\src',
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
  useQueueStore.getState().reset()
})

describe('QueueOverlay', () => {
  it('renders nothing when there is no queue work', () => {
    const { container } = render(<QueueOverlay />)
    expect(container).toBeEmptyDOMElement()
  })

  it('hydrates from the queue snapshot on mount', async () => {
    const snapshot: OpSnapshot[] = [{ progress: progress({}), conflict: null }]
    ipc.override('queue_snapshot', snapshot)

    render(<QueueOverlay />)
    expect(await screen.findByRole('button', { name: 'Expand transfer queue' })).toBeInTheDocument()
  })

  it('expands the collapsed toast into the panel and collapses again', async () => {
    const user = userEvent.setup()
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    render(<QueueOverlay />)

    const toast = await screen.findByRole('button', { name: 'Expand transfer queue' })
    await user.click(toast)

    expect(await screen.findByRole('region', { name: 'Transfer queue' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Fewer details/ }))
    expect(screen.queryByRole('region', { name: 'Transfer queue' })).not.toBeInTheDocument()
  })

  it('applies live progress events', async () => {
    ipc.override('queue_snapshot', [{ progress: progress({ progressPercent: 25 }), conflict: null }])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand transfer queue' })

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
    await screen.findByRole('button', { name: 'Expand transfer queue' })

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
      { progress: progress({ status: 'cancelled', currentFileName: null, bytesPerSecond: 0 }), conflict: null },
    ])
    render(<QueueOverlay />)

    await screen.findByRole('button', { name: 'Expand transfer queue' })
    await user.click(screen.getByRole('button', { name: 'Dismiss transfer queue' }))

    await waitFor(() => {
      expect(useQueueStore.getState().order).toEqual([])
    })
    expect(screen.queryByRole('button', { name: 'Expand transfer queue' })).not.toBeInTheDocument()
  })

  it('surfaces a conflict in the active pane only and keeps Enter mapped to Skip', async () => {
    const user = userEvent.setup()
    const resolveSpy = vi.fn(() => undefined)
    ipc.override('resolve_conflict', resolveSpy)
    act(() => {
      useQueueStore.setState({
        conflicts: {
          'op-1': {
            operationId: 'op-1',
            sourcePath: 'C:\\src\\a.txt',
            destinationPath: 'D:\\dst\\a.txt',
            name: 'a.txt',
          },
        },
        order: ['op-1'],
      })
    })

    render(
      <div className="grid grid-cols-2">
        <FilePane paneId="left" />
        <FilePane paneId="right" />
      </div>,
    )

    const dialog = await screen.findByRole('dialog', { name: 'Resolve file conflict' })
    expect(screen.getByLabelText('Left pane')).toContainElement(dialog)
    expect(screen.getByLabelText('Right pane')).not.toContainElement(dialog)

    await user.keyboard('{Enter}')
    expect(resolveSpy).toHaveBeenCalled()
  })

  it('Escape collapses the expanded panel rather than cancelling', async () => {
    const user = userEvent.setup()
    const cancelSpy = vi.fn(() => undefined)
    ipc.override('cancel_op', cancelSpy)
    ipc.override('queue_snapshot', [{ progress: progress({}), conflict: null }])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand transfer queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Transfer queue' })

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
    ipc.override('queue_snapshot', [
      { progress: progress({ status: 'active' }), conflict: null },
    ])
    render(<QueueOverlay />)
    await screen.findByRole('button', { name: 'Expand transfer queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Transfer queue' })

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
    await screen.findByRole('button', { name: 'Expand transfer queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Transfer queue' })

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
    await screen.findByRole('button', { name: 'Expand transfer queue' })

    act(() => {
      useQueueStore.getState().setExpanded(true)
    })
    await screen.findByRole('region', { name: 'Transfer queue' })

    const downButtons = screen.getAllByRole('button', { name: 'Move job down' })
    await user.click(downButtons[0])
    expect(reorderSpy).toHaveBeenCalled()
    await waitFor(() => {
      expect(useQueueStore.getState().order).toEqual(['op-2', 'op-1'])
    })
  })
})
