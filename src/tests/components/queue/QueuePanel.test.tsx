import { act, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { QueuePanel } from '@/components/queue/QueuePanel'
import type { OpProgress } from '@/lib/types/ipc'
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

function seed(operations: OpProgress[]) {
  act(() => {
    useQueueStore.setState({
      order: operations.map((operation) => operation.operationId),
      operations: Object.fromEntries(
        operations.map((operation) => [operation.operationId, operation]),
      ),
      expanded: true,
    })
  })
}

function row(title: string) {
  return screen.getByText(title).closest('.border-t') as HTMLElement
}

beforeEach(() => {
  useQueueStore.getState().reset()
})

describe('QueuePanel', () => {
  it('heads the card with the job count and collapses from the chevron', async () => {
    const user = userEvent.setup()
    seed([progress({}), progress({ operationId: 'op-2', status: 'pending' })])
    render(<QueuePanel />)

    expect(screen.getByRole('region', { name: 'Job queue' })).toHaveTextContent('2 jobs')
    await user.click(screen.getByRole('button', { name: 'Fewer details' }))

    expect(useQueueStore.getState().expanded).toBe(false)
  })

  it('lists waiting jobs with their items and a remove button', async () => {
    const user = userEvent.setup()
    const cancel = vi.fn(() => undefined)
    ipc.override('cancel_op', cancel)
    seed([
      progress({}),
      progress({
        operationId: 'op-2',
        status: 'pending',
        kind: 'move',
        totalItems: 32,
        itemNames: ['Season 01', 'poster.jpg'],
      }),
    ])
    render(<QueuePanel />)

    const waiting = row('Moving 32 items')
    expect(waiting).toHaveTextContent('Waiting · Season 01, poster.jpg, +30 more')
    const remove = within(waiting).getByRole('button', { name: 'Cancel job' })
    expect(remove).toHaveAttribute('title', 'Remove from queue')
    await user.click(remove)

    expect(cancel).toHaveBeenCalledWith({ id: 'op-2' })
  })

  it('reorders waiting jobs with the up and down buttons', async () => {
    const user = userEvent.setup()
    const reorder = vi.fn(() => undefined)
    ipc.override('reorder_ops', reorder)
    seed([
      progress({}),
      progress({ operationId: 'op-2', status: 'pending', totalItems: 2 }),
      progress({ operationId: 'op-3', status: 'pending', totalItems: 3 }),
    ])
    render(<QueuePanel />)

    const first = row('Copying 2 items')
    // Already first among the waiting jobs, so moving up changes nothing.
    await user.click(within(first).getByRole('button', { name: 'Move job up' }))
    expect(reorder).not.toHaveBeenCalled()
    await user.click(within(first).getByRole('button', { name: 'Move job down' }))

    expect(reorder).toHaveBeenCalledWith({ ids: ['op-3', 'op-2'] })
    expect(useQueueStore.getState().order).toEqual(['op-1', 'op-3', 'op-2'])
    await user.click(within(row('Copying 2 items')).getByRole('button', { name: 'Move job up' }))
    expect(useQueueStore.getState().order).toEqual(['op-1', 'op-2', 'op-3'])
  })

  it('offers pause, resume, retry and dismiss on the secondary rows', async () => {
    const user = userEvent.setup()
    const pause = vi.fn(() => undefined)
    const resume = vi.fn(() => undefined)
    const retry = vi.fn(() => undefined)
    ipc.override('pause_op', pause)
    ipc.override('resume_op', resume)
    ipc.override('retry_op', retry)
    seed([
      progress({ operationId: 'op-conflict', status: 'conflict', totalItems: 9 }),
      progress({ operationId: 'op-active', totalItems: 5 }),
      progress({ operationId: 'op-paused', status: 'paused', totalItems: 6 }),
      progress({ operationId: 'op-failed', status: 'failed', totalItems: 7 }),
      progress({ operationId: 'op-done', status: 'completed', totalItems: 8 }),
    ])
    render(<QueuePanel />)

    const active = row('Copying 5 items')
    expect(active).toHaveTextContent('· 25%')
    await user.click(within(active).getByRole('button', { name: 'Pause job' }))
    expect(pause).toHaveBeenCalledWith({ id: 'op-active' })

    await user.click(within(row('Copying 6 items')).getByRole('button', { name: 'Resume job' }))
    expect(resume).toHaveBeenCalledWith({ id: 'op-paused' })

    await user.click(within(row('Copying failed')).getByRole('button', { name: 'Retry job' }))
    expect(retry).toHaveBeenCalledWith({ id: 'op-failed' })

    await user.click(within(row('Copying complete')).getByRole('button', { name: 'Dismiss job' }))
    expect(useQueueStore.getState().operations['op-done']).toBeUndefined()
  })

  it('resolves a secondary conflict by keeping the panel open', async () => {
    const user = userEvent.setup()
    seed([
      progress({ operationId: 'op-active', totalItems: 5 }),
      progress({ operationId: 'op-conflict', status: 'conflict', totalItems: 9 }),
    ])
    act(() => useQueueStore.setState({ expanded: false }))
    render(<QueuePanel />)

    // A conflict outranks active work, so it takes the main card.
    expect(screen.getByRole('button', { name: 'Resolve conflict' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Resolve conflict' }))

    expect(useQueueStore.getState().expanded).toBe(true)
  })
})
