import type { FileCategory } from '@/lib/file-type'
import { getFileCategory, getFolderGlyphKind } from '@/lib/file-type'

function categoryOf(name: string, isDir = false): FileCategory {
  return getFileCategory({ name, isDir })
}

describe('getFileCategory', () => {
  it('classifies directories as folder regardless of name', () => {
    expect(categoryOf('Anything.zip', true)).toBe('folder')
    expect(categoryOf('Documents', true)).toBe('folder')
  })

  const representative: Array<[string, FileCategory]> = [
    ['main.ts', 'code'],
    ['script.PY', 'code'],
    ['build.wasm', 'code'],
    ['index.html', 'web'],
    ['styles.scss', 'web'],
    ['settings.json', 'config'],
    ['app.yaml', 'config'],
    ['data.csv', 'config'],
    ['notes.txt', 'text'],
    ['README.md', 'text'],
    ['captions.srt', 'text'],
    ['report.pdf', 'office'],
    ['sheet.xlsx', 'office'],
    ['book.epub', 'office'],
    ['photo.jpg', 'image'],
    ['vector.svg', 'image'],
    ['raw.cr2', 'image'],
    ['song.mp3', 'audio'],
    ['voice.opus', 'audio'],
    ['clip.mp4', 'video'],
    ['movie.mkv', 'video'],
    ['bundle.zip', 'archive'],
    ['app.jar', 'archive'],
    ['disk.iso', 'disc'],
    ['vm.qcow2', 'disc'],
    ['installer.exe', 'executable'],
    ['app.apk', 'executable'],
    ['lib.dll', 'executable'],
    ['font.woff2', 'font'],
    ['regular.ttf', 'font'],
    ['data.sqlite', 'database'],
    ['table.parquet', 'database'],
  ]

  it.each(representative)('maps %s to %s', (name, expected) => {
    expect(categoryOf(name)).toBe(expected)
  })

  it('covers every non-folder category at least once in the table', () => {
    const covered = new Set(representative.map(([, category]) => category))
    const all: FileCategory[] = [
      'code', 'web', 'config', 'text', 'office', 'image', 'audio', 'video',
      'archive', 'disc', 'executable', 'font', 'database',
    ]
    for (const category of all) {
      expect(covered.has(category)).toBe(true)
    }
  })

  it('matches the last extension of a multi-dot name', () => {
    expect(categoryOf('archive.tar.gz')).toBe('archive')
    expect(categoryOf('component.test.tsx')).toBe('code')
  })

  it('is case-insensitive on the extension', () => {
    expect(categoryOf('IMAGE.PNG')).toBe('image')
    expect(categoryOf('Mixed.JpEg')).toBe('image')
  })

  it('resolves known extensionless config names', () => {
    expect(categoryOf('Dockerfile')).toBe('config')
    expect(categoryOf('.gitignore')).toBe('config')
    expect(categoryOf('.editorconfig')).toBe('config')
    expect(categoryOf('.gitattributes')).toBe('config')
    expect(categoryOf('.dockerignore')).toBe('config')
  })

  it('falls back to generic for unknown, extensionless, and bare-dotfile names', () => {
    expect(categoryOf('Makefile')).toBe('generic')
    expect(categoryOf('LICENSE')).toBe('generic')
    expect(categoryOf('mystery.qwzzz')).toBe('generic')
    expect(categoryOf('.env')).toBe('generic')
    expect(categoryOf('noextension')).toBe('generic')
  })
})

describe('getFolderGlyphKind', () => {
  it('matches special folders case-insensitively', () => {
    expect(getFolderGlyphKind('Downloads', false)).toBe('downloads')
    expect(getFolderGlyphKind('downloads', true)).toBe('downloads')
    expect(getFolderGlyphKind('.git', false)).toBe('git')
    expect(getFolderGlyphKind('node_modules', true)).toBe('modules')
  })

  it('reflects open state for ordinary folders', () => {
    expect(getFolderGlyphKind('Documents', false)).toBe('closed')
    expect(getFolderGlyphKind('Documents', true)).toBe('open')
  })
})
