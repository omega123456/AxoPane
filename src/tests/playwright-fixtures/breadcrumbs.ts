import type { DirectoryEntry, ListDirResponse, SessionState } from '@/lib/types/ipc'

const baseEntry = {
  sizeBytes: null,
  itemCount: 4,
  typeLabel: 'Folder',
  modifiedAt: '2026-06-24T10:15:00Z',
  createdAt: '2026-06-12T10:15:00Z',
  attributes: [],
  isHidden: false,
  isSystem: false,
} satisfies Partial<DirectoryEntry>

function folder(name: string): DirectoryEntry {
  return {
    ...baseEntry,
    id: name,
    name,
    path: `C:\\Atlas\\Bravo\\Charlie\\Delta\\Echo\\Foxtrot\\${name}`,
    isDir: true,
  }
}

export const deepBreadcrumbSession: SessionState = {
  activePane: 'left',
  leftPath: 'C:\\Atlas\\Bravo\\Charlie\\Delta\\Echo\\Foxtrot',
  rightPath: 'C:\\Atlas\\Bravo\\Charlie\\Delta\\Echo\\Foxtrot',
}

export const deepBreadcrumbListDir: ListDirResponse = {
  path: 'C:\\Atlas\\Bravo\\Charlie\\Delta\\Echo\\Foxtrot',
  entries: [folder('Archives'), folder('Builds'), folder('Designs')],
}
