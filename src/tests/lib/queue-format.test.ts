import { describe, expect, it } from 'vitest'
import {
  formatItemPreview,
  formatJobProgress,
  formatJobTitle,
  jobFlags,
  verb,
} from '@/lib/queue-format'
import type { OpKind, OpProgress, OpStatus } from '@/lib/types/ipc'

function operationWithKind(kind: OpKind): OpProgress {
  return { kind } as OpProgress
}

function jobProgress(overrides: Partial<OpProgress> = {}): OpProgress {
  return {
    kind: 'copy',
    status: 'active' as OpStatus,
    totalItems: 2,
    completedItems: 0,
    totalBytes: 85_899_345_920,
    copiedBytes: 11_166_914_969,
    etaSeconds: 165,
    ...overrides,
  } as OpProgress
}

describe('verb', () => {
  it('maps each operation kind to its present-participle verb', () => {
    expect(verb(operationWithKind('copy'))).toBe('Copying')
    expect(verb(operationWithKind('move'))).toBe('Moving')
    expect(verb(operationWithKind('delete'))).toBe('Deleting')
    expect(verb(operationWithKind('compress'))).toBe('Compressing')
    expect(verb(operationWithKind('extract'))).toBe('Extracting')
  })
})

describe('formatItemPreview', () => {
  it('returns null when there are no item names', () => {
    expect(formatItemPreview([], 0)).toBeNull()
  })

  it('joins all names when total is 2 or fewer', () => {
    expect(formatItemPreview(['a.txt'], 1)).toBe('a.txt')
    expect(formatItemPreview(['a.txt', 'b.txt'], 2)).toBe('a.txt, b.txt')
  })

  it('previews the first two names and counts the rest as "+K more"', () => {
    expect(formatItemPreview(['a.txt', 'b.txt'], 5)).toBe('a.txt, b.txt, +3 more')
  })
})

describe('formatJobProgress', () => {
  it('reports job bytes and the time left for a transfer', () => {
    expect(formatJobProgress(jobProgress(), { scanning: false })).toEqual([
      '10.4 GB of 80.0 GB',
      'about 3 min left',
    ])
  })

  it('says the total is still growing while the backend discovers bytes', () => {
    expect(formatJobProgress(jobProgress(), { scanning: true })).toEqual([
      '10.4 GB of 80.0 GB (still scanning)',
    ])
  })

  it('replaces the estimate with "paused" so a stopped job never predicts a finish', () => {
    expect(formatJobProgress(jobProgress({ status: 'paused' }), { scanning: false })).toEqual([
      '10.4 GB of 80.0 GB',
      'paused',
    ])
  })

  it('says estimating until the backend has an ETA', () => {
    expect(formatJobProgress(jobProgress({ etaSeconds: null }), { scanning: false })).toEqual([
      '10.4 GB of 80.0 GB',
      'estimating…',
    ])
  })

  it('counts items rather than bytes for a delete, and ignores the scanning flag', () => {
    const deleting = jobProgress({
      kind: 'delete',
      totalItems: 500,
      completedItems: 412,
      etaSeconds: 8,
    })
    expect(formatJobProgress(deleting, { scanning: true })).toEqual([
      '412 of 500 items deleted',
      'about 8 sec left',
    ])
  })

  it('never counts more deleted items than the job has', () => {
    const deleting = jobProgress({ kind: 'delete', totalItems: 3, completedItems: 4 })
    expect(formatJobProgress(deleting, { scanning: false })[0]).toBe('3 of 3 items deleted')
  })
})

describe('formatJobTitle', () => {
  it('names how a finished job ended and how many items an unfinished one moves', () => {
    expect(formatJobTitle(jobProgress({ status: 'completed' }))).toBe('Copying complete')
    expect(formatJobTitle(jobProgress({ status: 'failed', kind: 'move' }))).toBe('Moving failed')
    expect(formatJobTitle(jobProgress({ status: 'cancelled', kind: 'delete' }))).toBe(
      'Deleting cancelled',
    )
    expect(formatJobTitle(jobProgress({ status: 'active', totalItems: 1200 }))).toBe(
      'Copying 1,200 items',
    )
  })
})

describe('jobFlags', () => {
  it('marks a running transfer as underway with a transfer rate', () => {
    expect(jobFlags(jobProgress({ status: 'active' }), false)).toMatchObject({
      isUnderway: true,
      isFinished: false,
      showsTransferRate: true,
      isConflict: false,
    })
  })

  it('reports no transfer rate for deletes and nothing underway once finished or pending', () => {
    expect(jobFlags(jobProgress({ kind: 'delete' }), false).showsTransferRate).toBe(false)
    expect(jobFlags(jobProgress({ status: 'pending' }), false)).toMatchObject({
      isPending: true,
      isUnderway: false,
    })
    expect(jobFlags(jobProgress({ status: 'failed' }), false)).toMatchObject({
      isFailed: true,
      isFinished: true,
      isUnderway: false,
    })
  })

  it('treats a queued conflict as a conflict even before the status changes', () => {
    expect(jobFlags(jobProgress({ status: 'active' }), true).isConflict).toBe(true)
    expect(jobFlags(jobProgress({ status: 'conflict' }), false).isConflict).toBe(true)
  })
})
