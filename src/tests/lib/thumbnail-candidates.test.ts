import {
  distanceFromVisibleRange,
  thumbnailCandidateFor,
  thumbnailCandidatesInRange,
  thumbnailPriority,
  type GraphicalThumbnailRange,
} from '@/lib/thumbnail-candidates'
import type { DirectoryEntry } from '@/lib/types/ipc'

const MODIFIED_AT = '2026-01-02T03:04:05Z'
const MODIFIED_UNIX_SECONDS = 1767323045

function file(name: string, overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    id: name,
    name,
    path: `/photos/${name}`,
    isDir: false,
    sizeBytes: 100,
    itemCount: null,
    typeLabel: 'Image',
    modifiedAt: MODIFIED_AT,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
    ...overrides,
  }
}

function range(direction: GraphicalThumbnailRange['direction']): GraphicalThumbnailRange {
  return { visibleStart: 2, visibleEnd: 4, prefetchStart: 0, prefetchEnd: 6, direction }
}

describe('thumbnailPriority', () => {
  it('ranks visible rows first, then rows ahead of the scroll direction', () => {
    expect(thumbnailPriority(3, range('down'))).toBe('visible')
    expect(thumbnailPriority(5, range('down'))).toBe('ahead')
    expect(thumbnailPriority(1, range('down'))).toBe('behind')
    expect(thumbnailPriority(1, range('up'))).toBe('ahead')
    expect(thumbnailPriority(5, range('up'))).toBe('behind')
  })
})

describe('distanceFromVisibleRange', () => {
  it('orders visible rows from the top and others by distance from the window', () => {
    expect(distanceFromVisibleRange(2, range('down'))).toBe(0)
    expect(distanceFromVisibleRange(4, range('down'))).toBe(2)
    expect(distanceFromVisibleRange(0, range('down'))).toBe(2)
    expect(distanceFromVisibleRange(6, range('down'))).toBe(2)
  })
})

describe('thumbnailCandidateFor', () => {
  it('builds a request for a sized, dated file', () => {
    expect(thumbnailCandidateFor(file('a.png'), 3, range('down'))).toEqual({
      path: '/photos/a.png',
      modifiedUnixSeconds: MODIFIED_UNIX_SECONDS,
      sizeBytes: 100,
      isDirectory: false,
      priority: 'visible',
      order: 1,
    })
  })

  it('skips folders, unsized files, undated files and unparseable dates', () => {
    expect(thumbnailCandidateFor(file('dir', { isDir: true }), 3, range('down'))).toBeNull()
    expect(thumbnailCandidateFor(file('b', { sizeBytes: null }), 3, range('down'))).toBeNull()
    expect(thumbnailCandidateFor(file('c', { modifiedAt: null }), 3, range('down'))).toBeNull()
    expect(thumbnailCandidateFor(file('d', { modifiedAt: 'never' }), 3, range('down'))).toBeNull()
  })
})

describe('thumbnailCandidatesInRange', () => {
  it('requests every eligible file in the prefetch window with its absolute index', () => {
    const entries = [file('0'), file('1', { isDir: true }), file('2'), file('3'), file('4')]
    const candidates = thumbnailCandidatesInRange(entries, {
      visibleStart: 2,
      visibleEnd: 2,
      prefetchStart: 1,
      prefetchEnd: 3,
      direction: 'stationary',
    })
    expect(candidates.map((candidate) => [candidate.path, candidate.priority])).toEqual([
      ['/photos/2', 'visible'],
      ['/photos/3', 'ahead'],
    ])
  })
})
