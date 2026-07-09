import { pathKey, pathsMatch } from '@/lib/path-compare'

describe('pathsMatch', () => {
  it('matches exact paths first', () => {
    expect(pathsMatch('/Users/AxoPane', '/Users/AxoPane')).toBe(true)
    expect(pathsMatch('C:\\Root\\File.txt', 'C:\\Root\\File.txt')).toBe(true)
  })

  it('falls back to case-insensitive matching only for Windows-style paths', () => {
    expect(pathsMatch('C:\\Root\\File.txt', 'c:\\root\\file.txt')).toBe(true)
    expect(pathsMatch('\\\\server\\share\\Folder', '\\\\SERVER\\SHARE\\folder')).toBe(true)
    expect(pathsMatch('/Users/AxoPane', '/users/axopane', 'case-sensitive')).toBe(false)
    expect(pathsMatch('/Work/Foo', '/Work/foo', 'case-sensitive')).toBe(false)
  })

  it('normalizes windows extended-length drive prefixes before fallback matching', () => {
    expect(pathsMatch('\\\\?\\C:\\Root\\File.txt', 'c:\\root\\file.txt')).toBe(true)
  })

  it('normalizes windows extended-length UNC prefixes before fallback matching', () => {
    expect(pathsMatch('\\\\?\\UNC\\server\\share\\Folder', '\\\\server\\share\\folder')).toBe(true)
  })

  it('does not match different paths', () => {
    expect(pathsMatch('/Users/AxoPane', '/Users/Elsewhere')).toBe(false)
    expect(pathsMatch('C:\\Root\\One', 'C:\\Root\\Two')).toBe(false)
  })

  it('keeps POSIX item-count dedupe keys case-sensitive', () => {
    expect(pathKey('/Work/Foo', 'case-sensitive')).not.toBe(pathKey('/Work/foo', 'case-sensitive'))
    expect(pathKey('C:\\Work\\Foo')).toBe(pathKey('c:\\work\\foo'))
  })

  it('uses macOS-only compatibility fallback for canonical/display path casing', () => {
    expect(pathsMatch('/Volumes/Work/Foo', '/volumes/work/foo', 'macos')).toBe(true)
    expect(pathKey('/Volumes/Work/Foo', 'macos')).toBe(pathKey('/volumes/work/foo', 'macos'))
    expect(pathsMatch('/Work/Foo', '/Work/foo', 'case-sensitive')).toBe(false)
  })
})
