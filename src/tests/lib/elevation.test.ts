import { afterEach, describe, expect, it } from 'vitest'
import { needsAdminRestart } from '@/lib/elevation'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

afterEach(() => {
  setPlatform(originalPlatform)
})

describe('needsAdminRestart', () => {
  it('matches the Windows permission error codes', () => {
    setPlatform('Win32')

    expect(needsAdminRestart('Access is denied. (os error 5)')).toBe(true)
    expect(needsAdminRestart('Failed to create folder: Zugriff verweigert. (os error 5)')).toBe(true)
    expect(needsAdminRestart('A required privilege is not held (os error 1314)')).toBe(true)
    expect(needsAdminRestart('Unknown { hresult: 0x80070005 }')).toBe(true)
  })

  it('ignores failures that more permissions cannot fix', () => {
    setPlatform('Win32')

    expect(needsAdminRestart('The system cannot find the file specified. (os error 2)')).toBe(false)
    expect(needsAdminRestart('Failed to copy: disk full (os error 112)')).toBe(false)
    expect(needsAdminRestart(null)).toBe(false)
    expect(needsAdminRestart('')).toBe(false)
  })

  it('never offers an elevated restart on macOS', () => {
    setPlatform('MacIntel')

    expect(needsAdminRestart('Permission denied (os error 5)')).toBe(false)
  })
})
