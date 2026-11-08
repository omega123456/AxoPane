import { describe, expect, it } from 'vitest'
import {
  footerText,
  scopeNoteFor,
  searchBackendInfo,
  withTrailingSeparator,
} from '@/components/search/search-palette-model'

const NO_RESULTS_FOOTER = {
  backend: 'everything' as const,
  status: null,
  searching: false,
  loaded: 0,
  total: 0,
  truncated: false,
}

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

  it('labels the macOS index footer by index state', () => {
    const macFooter = { ...NO_RESULTS_FOOTER, backend: 'macIndex' as const }
    expect(footerText(macFooter)).toBe('Folders on this Mac')
  })

  it('derives folders-only mode from the backend when capabilities are unknown', () => {
    expect(searchBackendInfo(null, null)).toEqual({ backend: 'none', foldersOnly: false })
  })

  it('asks for more characters before a query becomes searchable', () => {
    expect(scopeNoteFor(false, 'ab', false)).toContain('Keep typing')
    expect(scopeNoteFor(true, 'abc', true)).toBe('Folder names only on this Mac')
    expect(scopeNoteFor(true, 'abc', false)).toBe('')
  })
})
