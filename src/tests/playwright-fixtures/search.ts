import type { FolderSizeIndexStatus, SearchHit, SearchPage } from '@/lib/types/ipc'

/**
 * Search everywhere screenshot payloads: one page per palette state. Paths
 * are made-up examples; ranges are UTF-16 offsets into the name and parent.
 */
function hit(
  parentPath: string,
  name: string,
  isDir: boolean,
  modifiedAt: string,
  nameRanges: [number, number][],
  parentRanges: [number, number][] = [],
): SearchHit {
  const separator = parentPath.startsWith('/') ? '/' : '\\'
  return {
    path: `${parentPath}${separator}${name}`,
    name,
    parentPath,
    isDir,
    modifiedAt,
    nameRanges,
    parentRanges,
  }
}

const everything = { backend: 'everything', foldersOnly: false } as const

function windowsPage(items: SearchHit[], total = items.length): SearchPage {
  return { queryId: 1, offset: 0, total, truncated: false, items, capabilities: everything }
}

/** `report`: a name search across drives. */
export const searchNameResults = windowsPage([
  hit('D:\\Work\\Finance', 'Reports', true, '2025-11-04T08:02:00Z', [[0, 6]]),
  hit('C:\\Users\\alex\\Documents\\Finance', 'Q3 report.docx', false, '2025-11-03T16:40:00Z', [
    [3, 9],
  ]),
  hit('C:\\Users\\alex\\Downloads', 'report-2025-10.pdf', false, '2025-11-02T10:15:00Z', [[0, 6]]),
  hit('\\\\nas\\team\\Finance', 'Annual Reports', true, '2025-10-28T09:00:00Z', [[7, 13]]),
  hit('C:\\dev\\axopane\\src\\lib\\queue', 'report.ts', false, '2025-10-27T14:22:00Z', [[0, 6]]),
  hit(
    'C:\\Users\\alex\\Documents\\Finance\\2025',
    'expense-report.xlsx',
    false,
    '2025-10-19T11:05:00Z',
    [[8, 14]],
  ),
])

/** `alex/`: the children of every folder matching `alex`. */
export const searchBrowseResults = windowsPage([
  hit('C:\\Users\\alex', 'Desktop', true, '2025-11-05T07:30:00Z', [], [[9, 13]]),
  hit('C:\\Users\\alex', 'Documents', true, '2025-11-05T07:12:00Z', [], [[9, 13]]),
  hit('C:\\Users\\alex', 'Invoices', true, '2025-11-01T12:00:00Z', [], [[9, 13]]),
  hit('D:\\Libraries\\alex', 'invoice.pdf', false, '2025-10-30T15:45:00Z', [], [[13, 17]]),
  hit('C:\\Users\\alexios', 'Desktop', true, '2025-10-20T09:10:00Z', [], [[9, 13]]),
  hit('\\\\nas\\home\\alex.k', 'Invoices 2025', true, '2025-10-15T08:00:00Z', [], [[11, 15]]),
])

export const searchNoResults = windowsPage([])

/** Six of sixty, so the loading row shows at the end of the list. */
export const searchMoreResults = windowsPage(
  Array.from({ length: 6 }, (_, index) =>
    hit(
      'D:\\Work\\Clients\\Northwind\\Invoices\\2025',
      `invoice-${1000 + index}.pdf`,
      false,
      `2025-10-${String(20 - index).padStart(2, '0')}T09:00:00Z`,
      [[0, 7]],
    ),
  ),
  60,
)

/** `photo` on a Mac: folder names only. */
export const searchMacResults: SearchPage = {
  queryId: 1,
  offset: 0,
  total: 4,
  truncated: false,
  capabilities: { backend: 'macIndex', foldersOnly: true },
  items: [
    hit('/Users/alex/Desktop', 'photo-exports', true, '2025-11-04T18:20:00Z', [[0, 5]]),
    hit('/Users/alex/Documents/Hobbies', 'Photography', true, '2025-11-01T10:00:00Z', [[0, 5]]),
    hit('/Users/alex/Pictures', 'Photo Booth Library', true, '2025-10-22T13:30:00Z', [[0, 5]]),
    hit('/Users/alex/Projects/site/public', 'photos', true, '2025-10-09T17:45:00Z', [[0, 5]]),
  ],
}

export const searchMacIndexing: FolderSizeIndexStatus = {
  backend: 'macIndex',
  isAvailable: true,
  state: 'indexing',
  itemsScanned: 1_204_388,
}

export const everythingNotRunning: FolderSizeIndexStatus = {
  backend: 'everything',
  isAvailable: false,
  state: 'notReady',
}

export const searchFavourites = ['C:\\Users\\alex\\Desktop', 'C:\\Users\\alex\\Downloads']

export const searchRecentFolders = [
  'C:\\dev\\axopane\\src',
  'D:\\Photos\\2025',
  'C:\\Users\\alex\\Documents\\Finance',
]
