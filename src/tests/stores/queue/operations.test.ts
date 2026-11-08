import { describe, expect, it } from 'vitest'
import type { ConflictInfo, OpProgress } from '@/lib/types/ipc'
import {
  conflictedQueue,
  defaultQueueData,
  dismissedOperation,
  hydratedQueue,
  isTerminal,
  progressedQueue,
  removedOperation,
  retriedOperation,
  withOperationStatus,
  withoutConflict,
  type QueueData,
} from '@/stores/queue/operations'
import {
  CLEARED_THROUGHPUT_TRACK,
  foldedThroughputTrack,
  freshThroughputTrack,
  hydratedThroughputTrack,
  restartsThroughput,
  THROUGHPUT_EMA_FACTOR,
} from '@/stores/queue/throughput'

function progress(overrides: Partial<OpProgress> = {}): OpProgress {
  return {
    operationId: 'op',
    kind: 'copy',
    status: 'active',
    sourceDir: '/src',
    itemNames: ['a'],
    destinationDir: '/dst',
    totalItems: 10,
    completedItems: 4,
    totalBytes: 1000,
    copiedBytes: 400,
    progressPercent: 40,
    bytesPerSecond: 100,
    etaSeconds: 6,
    currentFileName: 'a',
    currentFileCopiedBytes: 0,
    currentFileTotalBytes: 0,
    errorMessage: null,
    ...overrides,
  }
}

const conflict: ConflictInfo = {
  operationId: 'op',
  sourcePath: '/src/a',
  destinationPath: '/dst/a',
  name: 'a',
}

function queueWith(operation: OpProgress): QueueData {
  return { ...defaultQueueData(), ...progressedQueue(defaultQueueData(), operation) }
}

describe('throughput track', () => {
  it('seeds a fresh track from the first reading', () => {
    expect(freshThroughputTrack(progress())).toEqual({
      history: [{ percent: 40, rate: 100 }],
      peak: 100,
      lastRawRate: 100,
      smoothedRate: 100,
    })
    expect(foldedThroughputTrack(CLEARED_THROUGHPUT_TRACK, progress()).history).toHaveLength(1)
  })

  it('steps the EMA only on a new raw rate and commits only on a new bucket', () => {
    const seeded = freshThroughputTrack(progress())
    const sameBucket = foldedThroughputTrack(
      seeded,
      progress({ progressPercent: 40.1, bytesPerSecond: 200 }),
    )
    expect(sameBucket.history).toBe(seeded.history)
    expect(sameBucket.smoothedRate).toBe(100 + 100 * THROUGHPUT_EMA_FACTOR)
    const heldRate = foldedThroughputTrack(
      sameBucket,
      progress({ progressPercent: 40.2, bytesPerSecond: 200 }),
    )
    expect(heldRate.smoothedRate).toBe(sameBucket.smoothedRate)
    const nextBucket = foldedThroughputTrack(
      heldRate,
      progress({ progressPercent: 60, bytesPerSecond: 200 }),
    )
    expect(nextBucket.history.at(-1)).toEqual({ percent: 60, rate: heldRate.smoothedRate })
    expect(nextBucket.peak).toBe(heldRate.smoothedRate)
  })

  it('keeps an in-flight track on hydrate', () => {
    const seeded = freshThroughputTrack(progress())
    expect(hydratedThroughputTrack(seeded, progress({ bytesPerSecond: 999 }))).toEqual(seeded)
    expect(hydratedThroughputTrack(undefined, progress()).peak).toBe(100)
  })

  it('restarts on any regression or a return to pending', () => {
    const previous = progress()
    expect(restartsThroughput(undefined, previous)).toBe(false)
    expect(restartsThroughput(previous, progress({ progressPercent: 50 }))).toBe(false)
    expect(restartsThroughput(previous, progress({ progressPercent: 10 }))).toBe(true)
    expect(restartsThroughput(previous, progress({ copiedBytes: 1 }))).toBe(true)
    expect(restartsThroughput(previous, progress({ completedItems: 1 }))).toBe(true)
    expect(restartsThroughput(previous, progress({ status: 'pending' }))).toBe(true)
  })
})

describe('queue operations', () => {
  it('appends progress once and resets the chart on a rewind', () => {
    const queue = queueWith(progress())
    expect(queue.order).toEqual(['op'])
    const rewound = progressedQueue(queue, progress({ progressPercent: 5, bytesPerSecond: 7 }))
    expect(rewound.order).toBe(queue.order)
    expect(rewound.throughput.op).toEqual(
      freshThroughputTrack(progress({ progressPercent: 5, bytesPerSecond: 7 })),
    )
  })

  it('keeps a conflict only while the operation stays in conflict', () => {
    const conflicted = {
      ...queueWith(progress()),
      ...conflictedQueue(queueWith(progress()), conflict),
    }
    expect(conflicted.expanded).toBe(true)
    expect(progressedQueue(conflicted, progress({ status: 'conflict' })).conflicts.op).toBe(
      conflict,
    )
    expect(progressedQueue(conflicted, progress()).conflicts.op).toBeUndefined()
    expect(withoutConflict(conflicted, 'op').conflicts).toEqual({})
    expect(conflictedQueue(defaultQueueData(), conflict).order).toEqual(['op'])
  })

  it('hydrates snapshots, keeping existing tracks', () => {
    const queue = queueWith(progress())
    const hydrated = hydratedQueue(queue, [
      { progress: progress(), conflict },
      { progress: progress({ operationId: 'other' }), conflict: null },
    ])
    expect(hydrated.order).toEqual(['op', 'other'])
    expect(hydrated.throughput.op).toEqual(queue.throughput.op)
    expect(Object.keys(hydrated.conflicts)).toEqual(['op'])
  })

  it('removes known operations and dismisses only terminal ones', () => {
    const queue = queueWith(progress())
    expect(removedOperation(queue, 'missing')).toBeNull()
    expect(removedOperation(queue, 'op')).toEqual({
      operations: {},
      conflicts: {},
      throughput: {},
      order: [],
    })
    expect(dismissedOperation(queue, 'op')).toBeNull()
    expect(dismissedOperation(queueWith(progress({ status: 'failed' })), 'op')?.order).toEqual([])
    expect(isTerminal('completed')).toBe(true)
    expect(isTerminal('active')).toBe(false)
  })

  it('updates status and clears the chart on retry', () => {
    const queue = queueWith(progress({ status: 'failed', errorMessage: 'boom' }))
    expect(withOperationStatus(queue, 'op', 'paused')?.operations?.op.status).toBe('paused')
    expect(withOperationStatus(queue, 'missing', 'paused')).toBeNull()
    const retried = retriedOperation(queue, 'op')
    expect(retried?.operations?.op).toMatchObject({ status: 'pending', errorMessage: null })
    expect(retried?.throughput?.op).toBe(CLEARED_THROUGHPUT_TRACK)
    expect(retriedOperation(queue, 'missing')).toBeNull()
  })
})
