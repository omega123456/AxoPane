import type { DirectoryEntry, ListDirResponse } from '@/lib/types/ipc'

const base = {
  sizeBytes: 4096,
  itemCount: null,
  modifiedAt: '2026-06-22T10:15:00Z',
  createdAt: '2026-06-10T10:15:00Z',
  attributes: [],
  isHidden: false,
  isSystem: false,
} satisfies Partial<DirectoryEntry>

function file(name: string, typeLabel: string): DirectoryEntry {
  return {
    ...base,
    id: name,
    name,
    path: `C:\\Users\\Omega\\${name}`,
    isDir: false,
    typeLabel,
  }
}

function folder(name: string): DirectoryEntry {
  return {
    ...base,
    id: name,
    name,
    path: `C:\\Users\\Omega\\${name}`,
    isDir: true,
    sizeBytes: null,
    itemCount: 7,
    typeLabel: 'Folder',
  }
}

/**
 * One entry per icon category (plus a special and an ordinary folder) so the
 * screenshot exercises every glyph + token color in a single pane.
 *
 * Rust is the sort authority for `list_dir` (directories first, then
 * natural-lexical by name), and the frontend no longer re-sorts a
 * non-size-sorted response — so this fixture lists entries in that same
 * pre-sorted order.
 */
export const fileTypesListDir: ListDirResponse = {
  path: 'C:\\Users\\Omega',
  entries: [
    folder('Downloads'),
    folder('Projects'),
    file('app.ts', 'TS file'),
    file('bundle.zip', 'ZIP archive'),
    file('clip.mp4', 'MP4 file'),
    file('data.sqlite', 'SQLite database'),
    file('disk.iso', 'Disc image'),
    file('index.html', 'HTML file'),
    file('installer.exe', 'Application'),
    file('mystery.qwzzz', 'File'),
    file('notes.txt', 'TXT file'),
    file('photo.png', 'PNG file'),
    file('regular.ttf', 'Font file'),
    file('report.pdf', 'PDF file'),
    file('settings.json', 'JSON file'),
    file('song.mp3', 'MP3 file'),
  ],
}
