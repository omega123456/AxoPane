import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import type { ConflictInfo, OpProgress, OpSnapshot } from '@/lib/types/ipc'
import {
  activeConflict,
  hasUnfinishedWork,
  isTerminal,
  orderedOperations,
  useQueueStore,
} from '@/stores/queue-store'

function progress(overrides: Partial<OpProgress>): OpProgress {
  return {
    operationId: 'op-1',
    kind: 'copy',
    status: 'active',
    sourceDir: 'C:\\src',
    destinationDir: 'D:\\dst',
    totalItems: 10,
    completedItems: 4,
    totalBytes: 1000,
    copiedBytes: 400,
    progressPercent: 40,
    bytesPerSecond: 100,
    etaSeconds: 6,
    currentFileName: 'a.txt',
    currentFileCopiedBytes: 50,
    currentFileTotalBytes: 100,
    errorMessage: null,
    ...overrides,
  }
}

function conflict(id: string): ConflictInfo {
  return {
    operationId: id,
    sourcePath: `C:\\src\\${id}.txt`,
    destinationPath: `D:\\dst\\${id}.txt`,
    name: `${id}.txt`,
  }
}

beforeEach(() => {
  ipc.install()
  useQueueStore.getState().reset()
})

describe('queue store', () => {
  it('hydrates operations, order and conflicts from a snapshot', () => {
    const snapshots: OpSnapshot[] = [
      { progress: progress({ operationId: 'op-1' }), conflict: null },
      { progress: progress({ operationId: 'op-2', status: 'conflict' }), conflict: conflict('op-2') },
    ]
    useQueueStore.getState().hydrate(snapshots)

    const state = useQueueStore.getState()
    expect(orderedOperations(state).map((operation) => operation.operationId)).toEqual([
      'op-1',
      'op-2',
    ])
    expect(activeConflict(state)?.operationId).toBe('op-2')
    expect(state.throughputHistory['op-1']).toEqual([100])
  })

  it('appends new operations on first progress and updates in place after', () => {
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', copiedBytes: 100 }))
    useQueueStore
      .getState()
      .applyProgress(progress({ operationId: 'op-1', copiedBytes: 900, bytesPerSecond: 275 }))

    const operations = orderedOperations(useQueueStore.getState())
    expect(operations).toHaveLength(1)
    expect(operations[0].copiedBytes).toBe(900)
    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual([100, 275])
  })

  it('clears a conflict when the op leaves the conflict state', () => {
    useQueueStore.getState().applyConflict(conflict('op-1'))
    expect(activeConflict(useQueueStore.getState())?.operationId).toBe('op-1')

    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', status: 'active' }))
    expect(activeConflict(useQueueStore.getState())).toBeUndefined()
  })

  it('expands the panel when a conflict arrives', () => {
    expect(useQueueStore.getState().expanded).toBe(false)
    useQueueStore.getState().applyConflict(conflict('op-1'))
    expect(useQueueStore.getState().expanded).toBe(true)
  })

  it('toggles expansion', () => {
    useQueueStore.getState().toggleExpanded()
    expect(useQueueStore.getState().expanded).toBe(true)
    useQueueStore.getState().toggleExpanded()
    expect(useQueueStore.getState().expanded).toBe(false)
  })

  it('pause/resume proxy to IPC and optimistically update status', () => {
    const pauseSpy = vi.fn()
    const resumeSpy = vi.fn()
    ipc.override('pause_op', () => {
      pauseSpy()
      return undefined
    })
    ipc.override('resume_op', () => {
      resumeSpy()
      return undefined
    })

    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', status: 'active' }))
    useQueueStore.getState().pause('op-1')
    expect(pauseSpy).toHaveBeenCalled()
    expect(useQueueStore.getState().operations['op-1'].status).toBe('paused')

    useQueueStore.getState().resume('op-1')
    expect(resumeSpy).toHaveBeenCalled()
    expect(useQueueStore.getState().operations['op-1'].status).toBe('active')
  })

  it('cancel proxies to IPC', () => {
    const cancelSpy = vi.fn()
    ipc.override('cancel_op', () => {
      cancelSpy()
      return undefined
    })
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1' }))
    useQueueStore.getState().cancel('op-1')
    expect(cancelSpy).toHaveBeenCalled()
  })

  it('retry resets a failed op to pending', () => {
    ipc.override('retry_op', () => undefined)
    useQueueStore.getState().applyProgress(
      progress({ operationId: 'op-1', status: 'failed', errorMessage: 'boom' }),
    )
    useQueueStore.getState().retry('op-1')
    const operation = useQueueStore.getState().operations['op-1']
    expect(operation.status).toBe('pending')
    expect(operation.errorMessage).toBeNull()
  })

  it('resolve clears the conflict locally', () => {
    ipc.override('resolve_conflict', () => undefined)
    useQueueStore.getState().applyConflict(conflict('op-1'))
    useQueueStore.getState().resolve('op-1', 'skip', false, null)
    expect(activeConflict(useQueueStore.getState())).toBeUndefined()
  })

  it('pause/resume/retry on unknown ids are safe no-ops', () => {
    ipc.override('pause_op', () => undefined)
    ipc.override('resume_op', () => undefined)
    ipc.override('retry_op', () => undefined)
    useQueueStore.getState().pause('missing')
    useQueueStore.getState().resume('missing')
    useQueueStore.getState().retry('missing')
    expect(orderedOperations(useQueueStore.getState())).toHaveLength(0)
  })

  it('removeOperation prunes the op, its order entry and any conflict', () => {
    useQueueStore.getState().applyConflict(conflict('op-1'))
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', status: 'completed' }))
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-2', status: 'active' }))

    useQueueStore.getState().removeOperation('op-1')

    const state = useQueueStore.getState()
    expect(state.order).toEqual(['op-2'])
    expect(state.operations['op-1']).toBeUndefined()
    expect(state.conflicts['op-1']).toBeUndefined()
    expect(state.throughputHistory['op-1']).toBeUndefined()
    expect(orderedOperations(state).map((operation) => operation.operationId)).toEqual(['op-2'])
  })

  it('removeOperation on an unknown id is a safe no-op', () => {
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', status: 'active' }))
    useQueueStore.getState().removeOperation('missing')
    expect(orderedOperations(useQueueStore.getState())).toHaveLength(1)
  })

  it('dismisses only terminal operations', () => {
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', status: 'cancelled' }))
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-2', status: 'active' }))

    useQueueStore.getState().dismissOperation('op-1')
    useQueueStore.getState().dismissOperation('op-2')

    expect(useQueueStore.getState().operations['op-1']).toBeUndefined()
    expect(useQueueStore.getState().operations['op-2']).toBeDefined()
  })

  it('reports terminal status and unfinished work', () => {
    expect(isTerminal('completed')).toBe(true)
    expect(isTerminal('active')).toBe(false)

    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', status: 'completed' }))
    expect(hasUnfinishedWork(useQueueStore.getState())).toBe(false)

    useQueueStore.getState().applyProgress(progress({ operationId: 'op-2', status: 'active' }))
    expect(hasUnfinishedWork(useQueueStore.getState())).toBe(true)
  })
})
