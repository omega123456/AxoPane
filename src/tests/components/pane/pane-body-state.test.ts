import { isPermissionError, paneBodyState } from '@/components/pane/pane-body-state'

const LOADED = { showSkeleton: false, loading: false, error: null, rowCount: 3 }

describe('isPermissionError', () => {
  it('recognises permission wording case-insensitively', () => {
    expect(isPermissionError('Access is denied.')).toBe(true)
    expect(isPermissionError('Operation not permitted: Permission')).toBe(true)
    expect(isPermissionError('Not found')).toBe(false)
    expect(isPermissionError(null)).toBe(false)
  })
})

describe('paneBodyState', () => {
  it('prefers the skeleton over every other state', () => {
    expect(paneBodyState({ ...LOADED, showSkeleton: true, error: 'denied' })).toBe('skeleton')
  })

  it('separates permission failures from other errors', () => {
    expect(paneBodyState({ ...LOADED, error: 'Access is denied' })).toBe('permissionDenied')
    expect(paneBodyState({ ...LOADED, error: 'Disk unplugged' })).toBe('error')
  })

  it('shows the empty state only when a finished listing has no rows', () => {
    expect(paneBodyState({ ...LOADED, rowCount: 0 })).toBe('empty')
    expect(paneBodyState({ ...LOADED, rowCount: 0, loading: true })).toBe('listing')
    expect(paneBodyState(LOADED)).toBe('listing')
  })
})
