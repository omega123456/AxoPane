import { describe, expect, it } from 'vitest'
import {
  announcementToPublish,
  isDiscoveringBytes,
  retainedCurrentFile,
} from '@/components/queue/job-view-state'

describe('job view state', () => {
  it('keeps the last known file through a cleared current file', () => {
    expect(retainedCurrentFile('a.txt', null)).toBe('a.txt')
    expect(retainedCurrentFile('a.txt', 'b.txt')).toBe('b.txt')
    expect(retainedCurrentFile(null, null)).toBeNull()
  })

  it('reports byte discovery while the total grows', () => {
    expect(isDiscoveringBytes(20, 10)).toBe(true)
    expect(isDiscoveringBytes(10, 10)).toBe(false)
  })

  it('publishes only a pending announcement that differs from the live one', () => {
    expect(announcementToPublish('next', 'current')).toBe('next')
    expect(announcementToPublish('current', 'current')).toBeNull()
    expect(announcementToPublish(null, 'current')).toBeNull()
  })
})
