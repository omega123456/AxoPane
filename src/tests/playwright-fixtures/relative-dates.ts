import type { DirectoryEntry, ListDirResponse } from '@/lib/types/ipc'

// The relative-dates screenshot pins the clock to this instant (see the spec),
// so each entry's `modifiedAt` lands in a known recency tier and renders a
// deterministic, colour-coded phrase.
export const RELATIVE_DATES_NOW = '2026-06-30T15:00:00Z'

const base = {
  sizeBytes: 4096,
  itemCount: null,
  createdAt: '2026-06-01T09:00:00Z',
  attributes: [],
  isHidden: false,
  isSystem: false,
} satisfies Partial<DirectoryEntry>

function file(name: string, typeLabel: string, modifiedAt: string): DirectoryEntry {
  return {
    ...base,
    id: name,
    name,
    path: `C:\\Users\\Omega\\${name}`,
    isDir: false,
    typeLabel,
    modifiedAt,
  }
}

/**
 * One entry per recency tier so the screenshot captures every relative tone:
 * recent (green), today (blue), yesterday (amber), and an aged item that falls
 * back to the absolute format (default tone).
 *
 * Rust is the sort authority for `list_dir` (natural-lexical by name) and the
 * frontend no longer re-sorts a non-size-sorted response, so entries are
 * listed in that same pre-sorted order.
 */
export const relativeDatesListDir: ListDirResponse = {
  path: 'C:\\Users\\Omega',
  entries: [
    file('archive.pdf', 'PDF file', '2026-06-10T10:15:00Z'), // beyond cutoff → absolute
    file('backup.zip', 'ZIP archive', '2026-06-29T13:00:00Z'), // 1 day ago
    file('build.log', 'LOG file', '2026-06-30T12:00:00Z'), // 3 hours ago
    file('draft.txt', 'TXT file', '2026-06-30T14:45:00Z'), // 15 minutes ago
  ],
}
