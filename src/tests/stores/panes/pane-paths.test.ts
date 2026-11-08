import { describe, expect, it } from 'vitest'
import { basenameFromPath, getParentPath } from '@/stores/panes/pane-paths'

describe('pane paths', () => {
  it('walks up POSIX, drive and UNC paths and stops at their roots', () => {
    expect(getParentPath('')).toBeNull()
    expect(getParentPath('/')).toBeNull()
    expect(getParentPath('/Users')).toBe('/')
    expect(getParentPath('/Users/omega/')).toBe('/Users')
    expect(getParentPath('relative')).toBeNull()
    expect(getParentPath('C:\\Users')).toBe('C:\\')
    expect(getParentPath('D:/Media/Shows')).toBe('D:/Media')
    expect(getParentPath('\\\\?\\C:\\Users\\Omega')).toBe('C:\\Users')
    expect(getParentPath('\\\\nas\\media')).toBeNull()
  })

  it('names the last segment, falling back to the whole root', () => {
    expect(basenameFromPath('/Users/omega/')).toBe('omega')
    expect(basenameFromPath('C:\\Users\\Omega')).toBe('Omega')
    expect(basenameFromPath('C:')).toBe('C:')
    expect(basenameFromPath('/')).toBe('/')
  })
})
