import type {
  DirectoryEntry,
  ListDirResponse,
  RequestThumbnailsRequest,
  SessionState,
  ThumbnailResultEvent,
} from '@/lib/types/ipc'

const root = 'C:\\Users\\Omega\\Visual fixtures'

const base = {
  itemCount: null,
  modifiedAt: '2026-07-12T09:30:00Z',
  createdAt: '2026-07-01T09:30:00Z',
  attributes: [],
  isHidden: false,
  isSystem: false,
} satisfies Partial<DirectoryEntry>

function folder(id: string, name: string): DirectoryEntry {
  return {
    ...base,
    id,
    name,
    path: `${root}\\${name}`,
    isDir: true,
    sizeBytes: null,
    itemCount: 8,
    typeLabel: 'Folder',
  }
}

function file(id: string, name: string, typeLabel: string, sizeBytes = 4_096): DirectoryEntry {
  return {
    ...base,
    id,
    name,
    path: `${root}\\${name}`,
    isDir: false,
    sizeBytes,
    typeLabel,
  }
}

/** A compact, stable listing that deliberately exercises graphical-only states. */
export const paneViewListDir: ListDirResponse = {
  path: root,
  entries: [
    folder('assets', 'Assets'),
    folder('designs', 'Design references'),
    file(
      'wide-photo',
      'A very long photograph filename that truncates in a narrow icon tile.png',
      'PNG file',
      49_152,
    ),
    file('preview', 'Mountain preview.png', 'PNG file', 65_536),
    file('fallback', 'Unsupported artwork.psd', 'PSD file', 131_072),
    file('failure', 'Unreadable scan.tiff', 'TIFF file', 98_304),
    {
      ...file('hidden', 'Hidden reference.txt', 'TXT file'),
      isHidden: true,
      attributes: ['hidden'],
    },
  ],
}

export const iconsSession: SessionState = {
  activePane: 'left',
  leftPath: root,
  rightPath: 'D:\\projects',
  left: {
    activeTabIndex: 0,
    tabs: [
      { path: root, sortKey: 'name', sortDirection: 'asc', filter: '', viewMode: 'icons' },
      {
        path: 'C:\\Users\\Omega',
        sortKey: 'name',
        sortDirection: 'asc',
        filter: '',
        viewMode: 'details',
      },
    ],
  },
}

export const thumbnailsSession: SessionState = {
  ...iconsSession,
  left: {
    activeTabIndex: 0,
    tabs: [
      { path: root, sortKey: 'name', sortDirection: 'asc', filter: '', viewMode: 'thumbnails' },
    ],
  },
}

export type ThumbnailFixture = {
  outcomes: Record<string, { state: ThumbnailResultEvent['state']; dataUrl: string | null }>
}

// A tiny valid transparent PNG. It keeps preview rendering deterministic without
// asking the host operating system for native artwork.
const transparentPreview =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Jv0YAAAAASUVORK5CYII='

export const thumbnailFixture: ThumbnailFixture = {
  outcomes: {
    'Mountain preview.png': { state: 'ready', dataUrl: transparentPreview },
    'A very long photograph filename that truncates in a narrow icon tile.png': {
      state: 'ready',
      dataUrl: transparentPreview,
    },
    'Unsupported artwork.psd': { state: 'unavailable', dataUrl: null },
    'Unreadable scan.tiff': { state: 'failed', dataUrl: null },
  },
}

/** Build contextual results from fixture-owned data; the router only transports them. */
export function resolveThumbnailFixtureBatch(
  request: RequestThumbnailsRequest,
  fixture: ThumbnailFixture,
): ThumbnailResultEvent[] {
  return request.candidates.map((candidate) => {
    const outcome = fixture.outcomes[candidate.path.split('\\').at(-1) ?? '']
    return {
      paneId: request.paneId,
      tabId: request.tabId,
      path: request.path,
      generation: request.generation,
      fingerprintPath: candidate.path,
      modifiedUnixSeconds: candidate.modifiedUnixSeconds,
      sizeBytes: candidate.sizeBytes,
      state: outcome?.state ?? 'unavailable',
      quality: outcome?.state === 'ready' ? 'high' : null,
      dataUrl: outcome?.dataUrl ?? null,
    }
  })
}
