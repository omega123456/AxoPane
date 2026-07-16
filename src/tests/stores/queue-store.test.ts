import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import type { ConflictInfo, OpProgress, OpSnapshot, ThroughputSample } from '@/lib/types/ipc'
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
    itemNames: ['a.txt'],
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

function sample(percent: number, rate: number): ThroughputSample {
  return { percent, rate }
}

beforeEach(() => {
  ipc.install()
  useQueueStore.getState().reset()
})

describe('queue store', () => {
  it('hydrates operations, order and conflicts from a snapshot', () => {
    const snapshots: OpSnapshot[] = [
      { progress: progress({ operationId: 'op-1' }), conflict: null },
      {
        progress: progress({ operationId: 'op-2', status: 'conflict' }),
        conflict: conflict('op-2'),
      },
    ]
    useQueueStore.getState().hydrate(snapshots)

    const state = useQueueStore.getState()
    expect(orderedOperations(state).map((operation) => operation.operationId)).toEqual([
      'op-1',
      'op-2',
    ])
    expect(activeConflict(state)?.operationId).toBe('op-2')
    expect(state.throughputHistory['op-1']).toEqual([sample(40, 100)])
    expect(state.throughputPeak['op-1']).toBe(100)
  })

  it('opens a new smoothed bucket when progress advances into a new bucket', () => {
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', copiedBytes: 100 }))
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'op-1',
        copiedBytes: 900,
        progressPercent: 90,
        bytesPerSecond: 300,
      }),
    )

    const operations = orderedOperations(useQueueStore.getState())
    expect(operations).toHaveLength(1)
    expect(operations[0].copiedBytes).toBe(900)
    // The new bucket opens at the EMA-smoothed rate (smoothing carries across
    // the boundary so the leading edge never jumps): 100 + (300 - 100)*0.25 = 150.
    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual([
      sample(40, 100),
      sample(90, 150),
    ])
    // Peak tracks the displayed (smoothed) rate, non-decreasing.
    expect(useQueueStore.getState().throughputPeak['op-1']).toBe(150)
  })

  it('ratchets the throughput peak up only — a stall never drops it', () => {
    useQueueStore
      .getState()
      .applyProgress(progress({ operationId: 'op-1', progressPercent: 40, bytesPerSecond: 100 }))
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'op-1',
        progressPercent: 60,
        copiedBytes: 600,
        bytesPerSecond: 600,
      }),
    )
    // The burst smooths to 100 + (600 - 100)*0.25 = 225.
    expect(useQueueStore.getState().throughputPeak['op-1']).toBe(225)

    // A non-advancing stall EMA-steps the active bucket down
    // (225 + (0 - 225)*0.25 = 168.75); the frozen peak must hold at the earlier
    // high (225) so the Y ceiling never drops.
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'op-1',
        progressPercent: 60,
        copiedBytes: 600,
        bytesPerSecond: 0,
      }),
    )
    // A stall inside the same bucket never touches the frozen plotted point and
    // never drops the peak; the drop would only ever surface as a *new* frozen
    // point once progress crosses into the next bucket.
    const state = useQueueStore.getState()
    expect(state.throughputHistory['op-1'].at(-1)).toEqual(sample(60, 225))
    expect(state.throughputPeak['op-1']).toBe(225)
  })

  it('holds the plotted point within a bucket and commits the carried EMA when it advances', () => {
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', progressPercent: 40 }))
    useQueueStore
      .getState()
      .applyProgress(progress({ operationId: 'op-1', progressPercent: 40, bytesPerSecond: 200 }))

    // Within a bucket the EMA steps internally (100 -> 125) but the plotted point
    // is untouched — there is no provisional point that moves as it settles.
    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual([sample(40, 100)])

    // Crossing into a new bucket commits one frozen point at the carried EMA
    // value (125), gated so the repeated 200 B/s reading does not step it again.
    useQueueStore
      .getState()
      .applyProgress(progress({ operationId: 'op-1', progressPercent: 60, bytesPerSecond: 200 }))
    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual([
      sample(40, 100),
      sample(60, 125),
    ])
  })

  it('resets throughput history when percent or counters regress', () => {
    useQueueStore
      .getState()
      .applyProgress(
        progress({ operationId: 'op-1', progressPercent: 40, copiedBytes: 400, completedItems: 4 }),
      )
    useQueueStore
      .getState()
      .applyProgress(
        progress({ operationId: 'op-1', progressPercent: 70, copiedBytes: 700, completedItems: 7 }),
      )

    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'op-1',
        progressPercent: 12,
        copiedBytes: 120,
        completedItems: 1,
        bytesPerSecond: 90,
      }),
    )

    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual([sample(12, 90)])
    expect(useQueueStore.getState().throughputPeak['op-1']).toBe(90)
  })

  it('keeps delete throughput history when progress is monotonic', () => {
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'delete-1',
        kind: 'delete',
        itemNames: ['folder'],
        totalItems: 1,
        completedItems: 0,
        totalBytes: 100,
        copiedBytes: 0,
        progressPercent: 0,
        bytesPerSecond: 0,
        currentFileName: null,
        currentFileCopiedBytes: 0,
        currentFileTotalBytes: 0,
      }),
    )
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'delete-1',
        kind: 'delete',
        itemNames: ['folder'],
        totalItems: 1,
        completedItems: 0,
        totalBytes: 100,
        copiedBytes: 40,
        progressPercent: 40,
        bytesPerSecond: 400,
        currentFileName: null,
        currentFileCopiedBytes: 0,
        currentFileTotalBytes: 0,
      }),
    )
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'delete-1',
        kind: 'delete',
        itemNames: ['folder'],
        totalItems: 1,
        completedItems: 1,
        totalBytes: 100,
        copiedBytes: 100,
        progressPercent: 100,
        bytesPerSecond: 100,
        currentFileName: null,
        currentFileCopiedBytes: 0,
        currentFileTotalBytes: 0,
      }),
    )

    expect(useQueueStore.getState().throughputHistory['delete-1']).toEqual([
      sample(0, 0),
      sample(40, 100),
      sample(100, 100),
    ])
  })

  it('preserves existing throughput history when hydrate runs again', () => {
    useQueueStore.getState().applyProgress(progress({ operationId: 'op-1', progressPercent: 40 }))
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'op-1',
        progressPercent: 70,
        bytesPerSecond: 200,
      }),
    )

    const existingHistory = useQueueStore.getState().throughputHistory['op-1']
    const existingPeak = useQueueStore.getState().throughputPeak['op-1']
    useQueueStore
      .getState()
      .hydrate([
        {
          progress: progress({ operationId: 'op-1', progressPercent: 75, bytesPerSecond: 150 }),
          conflict: null,
        },
      ])

    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual(existingHistory)
    // Hydrate never lowers the peak (here the snapshot rate is below it).
    expect(useQueueStore.getState().throughputPeak['op-1']).toBe(existingPeak)
  })

  it('buckets samples by progress so history stays bounded no matter how many events arrive', () => {
    for (let index = 0; index <= 1000; index += 1) {
      useQueueStore.getState().applyProgress(
        progress({
          operationId: 'op-1',
          progressPercent: index / 10,
          copiedBytes: index,
          completedItems: index,
          bytesPerSecond: 200,
        }),
      )
    }

    const history = useQueueStore.getState().throughputHistory['op-1']
    // 1001 advancing events collapse into at most one sample per progress bucket.
    expect(history.length).toBeGreaterThan(1)
    expect(history.length).toBeLessThanOrEqual(90)
    for (let index = 1; index < history.length; index += 1) {
      expect(history[index].percent).toBeGreaterThanOrEqual(history[index - 1].percent)
    }
  })

  it('commits one frozen point per bucket and never mutates it within the bucket', () => {
    const apply = (percent: number, rate: number) =>
      useQueueStore.getState().applyProgress(
        progress({
          operationId: 'op-1',
          progressPercent: percent,
          copiedBytes: percent,
          completedItems: Math.floor(percent),
          bytesPerSecond: rate,
        }),
      )

    apply(5, 100)
    apply(10, 500) // crosses into a new bucket → commit a frozen point
    const afterTwo = useQueueStore.getState().throughputHistory['op-1']
    expect(afterTwo).toHaveLength(2)

    // Further events inside the same bucket as 10% neither append nor edit any
    // plotted point — the whole history stays byte-for-byte identical, so the
    // chart never draws a point that later moves as it settles.
    apply(10.01, 999)
    apply(10.02, 999)
    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual(afterTwo)
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

  it('skip proxies to IPC', () => {
    const skipSpy = vi.fn()
    ipc.override('skip_op', () => {
      skipSpy()
      return undefined
    })
    useQueueStore.getState().skip('op-1')
    expect(skipSpy).toHaveBeenCalled()
  })

  it('retry resets a failed op to pending', () => {
    ipc.override('retry_op', () => undefined)
    useQueueStore
      .getState()
      .applyProgress(progress({ operationId: 'op-1', status: 'failed', errorMessage: 'boom' }))
    useQueueStore.getState().applyProgress(
      progress({
        operationId: 'op-1',
        progressPercent: 85,
        copiedBytes: 850,
        completedItems: 8,
      }),
    )
    useQueueStore.getState().retry('op-1')
    const operation = useQueueStore.getState().operations['op-1']
    expect(operation.status).toBe('pending')
    expect(operation.errorMessage).toBeNull()
    expect(useQueueStore.getState().throughputHistory['op-1']).toEqual([])
    expect(useQueueStore.getState().throughputPeak['op-1']).toBe(0)
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
    expect(state.throughputPeak['op-1']).toBeUndefined()
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
    expect(useQueueStore.getState().throughputHistory['op-1']).toBeUndefined()
    expect(useQueueStore.getState().throughputHistory['op-2']).toBeDefined()
    expect(useQueueStore.getState().throughputPeak['op-1']).toBeUndefined()
    expect(useQueueStore.getState().throughputPeak['op-2']).toBeDefined()
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
