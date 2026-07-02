import { describe, expect, it } from 'vitest'
import {
  buildWarmRequestForEntry,
  extensionOf,
  nativeMenuTypeKeyForEntry,
} from '@/lib/context-menu/native-menu-type-key'
import type { DirectoryEntry } from '@/lib/types/ipc'

function entryAt(path: string, isDir = false): DirectoryEntry {
  const name = path.split(/[/\\]/).filter(Boolean).at(-1) ?? path
  return {
    id: path,
    name,
    path,
    isDir,
    iconDataUrl: null,
    sizeBytes: isDir ? null : 100,
    itemCount: isDir ? 0 : null,
    typeLabel: isDir ? 'Folder' : 'File',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

describe('extensionOf', () => {
  it('extracts the lowercased extension of the final path component', () => {
    expect(extensionOf('C:\\root\\Report.PDF')).toBe('pdf')
    expect(extensionOf('/home/user/report.pdf')).toBe('pdf')
  })

  it('treats a component with no dot as extensionless', () => {
    expect(extensionOf('C:\\root\\Documents')).toBe('')
  })

  it('uses only the final dot-segment for multi-dot names', () => {
    expect(extensionOf('C:\\root\\archive.tar.gz')).toBe('gz')
  })

  it('treats a leading-dot name as extensionless', () => {
    expect(extensionOf('C:\\root\\.gitignore')).toBe('')
  })

  it('treats a trailing-dot name as extensionless', () => {
    expect(extensionOf('C:\\root\\archive.')).toBe('')
  })

  it('considers only the final path component, ignoring dots in parent directories', () => {
    expect(extensionOf('C:\\root.old\\readme')).toBe('')
    expect(extensionOf('C:\\root.old\\readme.txt')).toBe('txt')
  })

  it('resolves a dot that is not the first character even with a leading dot elsewhere', () => {
    expect(extensionOf('..gitignore')).toBe('gitignore')
  })
})

describe('nativeMenuTypeKeyForEntry', () => {
  it('derives file::<ext> for a file entry', () => {
    expect(nativeMenuTypeKeyForEntry(entryAt('C:\\root\\Report.pdf'))).toBe('file::pdf')
  })

  it('derives file::<empty> for an extensionless file', () => {
    expect(nativeMenuTypeKeyForEntry(entryAt('C:\\root\\README'))).toBe('file::')
  })

  it('derives folder::<ext> for a folder with a dotted name', () => {
    expect(nativeMenuTypeKeyForEntry(entryAt('C:\\root\\My.Project', true))).toBe('folder::project')
  })

  it('derives folder:: for a plain folder name', () => {
    expect(nativeMenuTypeKeyForEntry(entryAt('C:\\root\\Documents', true))).toBe('folder::')
  })
})

describe('buildWarmRequestForEntry', () => {
  it('builds a full LoadNativeMenuRequest for a file, mirroring the interactive single-file request', () => {
    const entry = entryAt('C:\\root\\Report.pdf')
    const request = buildWarmRequestForEntry(entry, 'C:\\root', 'warm:file::pdf')

    expect(request).toEqual({
      requestId: 'warm:file::pdf',
      targetKind: 'file',
      targetPath: 'C:\\root\\Report.pdf',
      folderPath: 'C:\\root',
      selectedPaths: ['C:\\root\\Report.pdf'],
    })
  })

  it('builds a full LoadNativeMenuRequest for a folder, mirroring the interactive single-folder request', () => {
    const entry = entryAt('C:\\root\\Documents', true)
    const request = buildWarmRequestForEntry(entry, 'C:\\root', 'warm:folder::')

    expect(request).toEqual({
      requestId: 'warm:folder::',
      targetKind: 'folder',
      targetPath: 'C:\\root\\Documents',
      folderPath: 'C:\\root',
      selectedPaths: ['C:\\root\\Documents'],
    })
  })
})
