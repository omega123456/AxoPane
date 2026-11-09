import { describe, expect, it } from 'vitest'
import {
  FILE_NAMES_READY_ANNOUNCEMENT,
  footerText,
  indexAnnouncementAfter,
  scopeNoteFor,
  searchBackendInfo,
  withTrailingSeparator,
  type SearchFooterInput,
} from '@/components/search/search-palette-model'
import type { FolderSizeIndexStatus } from '@/lib/types/ipc'

const NO_RESULTS_FOOTER: SearchFooterInput = {
  backend: 'everything',
  phase: 'complete',
  status: null,
  searching: false,
  loaded: 0,
  total: 0,
  truncated: false,
}

const ONE_RESULT = { searching: true, loaded: 1, total: 1 }

function macStatus(overrides: Partial<FolderSizeIndexStatus> = {}): FolderSizeIndexStatus {
  return {
    backend: 'macIndex',
    isAvailable: true,
    state: 'upToDate',
    fileNamesAvailable: true,
    ...overrides,
  }
}

const MAC_PAGE = { backend: 'macIndex', foldersOnly: false } as const

describe('search palette model', () => {
  it('appends the platform separator only when missing', () => {
    expect(withTrailingSeparator('/Users/alex')).toBe('/Users/alex/')
    expect(withTrailingSeparator('/Users/alex/')).toBe('/Users/alex/')
    expect(withTrailingSeparator('C:\\Users')).toBe('C:\\Users\\')
    expect(withTrailingSeparator('C:\\Users\\')).toBe('C:\\Users\\')
  })

  it('describes partial and truncated result pages in the footer', () => {
    const partial = { ...NO_RESULTS_FOOTER, searching: true, loaded: 5, total: 9 }
    expect(footerText(partial)).toContain('Showing 5 of 9')
    expect(footerText({ ...partial, loaded: 1, total: 1 })).toContain('1 result')
    expect(footerText({ ...partial, truncated: true })).toContain('refine your search')
    expect(footerText(NO_RESULTS_FOOTER)).toBe('Everything')
  })

  it('labels the macOS index footer by palette phase', () => {
    const macFooter = { ...NO_RESULTS_FOOTER, backend: 'macIndex' as const, ...ONE_RESULT }
    expect(footerText(macFooter)).toBe('Files and folders on this Mac · 1 result')
    expect(footerText({ ...macFooter, phase: 'updating' })).toBe('Updating index… · 1 result')
    expect(footerText({ ...macFooter, phase: 'fallback' })).toBe('Folders on this Mac · 1 result')
    const indexing = { ...macFooter, phase: 'indexing' as const }
    expect(footerText(indexing)).toBe('Indexing files…')
    const counted = macStatus({ state: 'indexing', itemsScanned: 1_204_388 })
    expect(footerText({ ...indexing, status: counted })).toBe('Indexing files… 1.2M items')
    const paused = macStatus({ state: 'paused', pauseReason: 'thermal' })
    expect(footerText({ ...macFooter, phase: 'paused', status: paused })).toBe(
      'Paused to keep your Mac cool',
    )
    expect(footerText({ ...macFooter, phase: 'paused' })).toBe('Paused in Low Power Mode')
  })

  it('derives the palette phase from the index status', () => {
    expect(searchBackendInfo(null, null)).toEqual({
      backend: 'none',
      foldersOnly: false,
      phase: 'complete',
    })
    expect(searchBackendInfo(null, macStatus())).toEqual({
      backend: 'macIndex',
      foldersOnly: false,
      phase: 'complete',
    })
    expect(searchBackendInfo(null, macStatus({ state: 'updating' })).phase).toBe('updating')
    expect(searchBackendInfo(null, macStatus({ state: 'paused' })).phase).toBe('paused')
    const firstScan = macStatus({ state: 'indexing', fileNamesAvailable: false })
    expect(searchBackendInfo(null, firstScan)).toEqual({
      backend: 'macIndex',
      foldersOnly: true,
      phase: 'indexing',
    })
    const namesUnreadable = macStatus({ state: 'updating', fileNamesAvailable: false })
    expect(searchBackendInfo(null, namesUnreadable).phase).toBe('fallback')
  })

  it('prefers the live index status over the last page capabilities', () => {
    const fallbackPage = { ...MAC_PAGE, foldersOnly: true }
    expect(searchBackendInfo(fallbackPage, macStatus()).foldersOnly).toBe(false)
    expect(searchBackendInfo(fallbackPage, null)).toEqual({
      backend: 'macIndex',
      foldersOnly: true,
      phase: 'fallback',
    })
    expect(searchBackendInfo(MAC_PAGE, null).phase).toBe('complete')
  })

  it('picks the scope note by priority', () => {
    expect(scopeNoteFor(false, 'ab', 'indexing')).toContain('Keep typing')
    expect(scopeNoteFor(true, 'abc', 'indexing')).toBe(
      'Results may be incomplete until indexing finishes.',
    )
    expect(scopeNoteFor(true, 'abc', 'paused')).toBe(
      'Results may be incomplete until indexing resumes.',
    )
    expect(scopeNoteFor(true, 'abc', 'fallback')).toBe(
      'File names aren’t available on this Mac right now.',
    )
    expect(scopeNoteFor(true, 'abc', 'updating')).toBe('')
    expect(scopeNoteFor(true, 'abc', 'complete')).toBe('')
  })

  it('announces file names once when indexing finishes', () => {
    const indexing = macStatus({ state: 'indexing' })
    const done = macStatus()
    expect(indexAnnouncementAfter(indexing, done, '')).toBe(FILE_NAMES_READY_ANNOUNCEMENT)
    expect(indexAnnouncementAfter(indexing, indexing, '')).toBe('')
    expect(indexAnnouncementAfter(done, indexing, FILE_NAMES_READY_ANNOUNCEMENT)).toBe('')
    expect(indexAnnouncementAfter(done, done, FILE_NAMES_READY_ANNOUNCEMENT)).toBe(
      FILE_NAMES_READY_ANNOUNCEMENT,
    )
    const withoutNames = macStatus({ fileNamesAvailable: false })
    expect(indexAnnouncementAfter(indexing, withoutNames, '')).toBe('')
    expect(indexAnnouncementAfter(null, null, '')).toBe('')
  })
})
