import type {
  ActiveItemsSortResponse,
  IpcEventMap,
  ListDirResponse,
  SessionState,
  VisibleItemCountsRequest,
} from '@/lib/types/ipc'

export const itemsSortPendingSession: SessionState = {
  activePane: 'left',
  leftPath: 'C:\\Users\\Omega',
  rightPath: 'D:\\projects',
  left: {
    activeTabIndex: 0,
    tabs: [{ path: 'C:\\Users\\Omega', sortKey: 'items', sortDirection: 'desc', filter: '' }],
  },
}

export const itemsSortPendingListDir: ListDirResponse = {
  path: 'C:\\Users\\Omega',
  entries: [
    {
      id: 'docs',
      name: 'Documents',
      path: 'C:\\Users\\Omega\\Documents',
      isDir: true,
      sizeBytes: null,
      itemCount: null,
      typeLabel: 'Folder',
      modifiedAt: '2026-06-20T10:15:00Z',
      createdAt: '2026-06-01T10:15:00Z',
      attributes: [],
      isHidden: false,
      isSystem: false,
    },
    {
      id: 'designs',
      name: 'Designs',
      path: 'C:\\Users\\Omega\\Designs',
      isDir: true,
      sizeBytes: null,
      itemCount: null,
      typeLabel: 'Folder',
      modifiedAt: '2026-06-18T10:15:00Z',
      createdAt: '2026-05-28T10:15:00Z',
      attributes: [],
      isHidden: false,
      isSystem: false,
    },
    {
      id: 'report',
      name: 'Report.txt',
      path: 'C:\\Users\\Omega\\Report.txt',
      isDir: false,
      sizeBytes: 2048,
      itemCount: null,
      typeLabel: 'TXT file',
      modifiedAt: '2026-06-22T10:15:00Z',
      createdAt: '2026-06-10T10:15:00Z',
      attributes: [],
      isHidden: false,
      isSystem: false,
    },
  ],
}

export const itemsSortReadyResponse: ActiveItemsSortResponse = {
  kind: 'ready',
  context: {
    paneId: 'left',
    tabId: 'left-tab-1',
    requestId: 1,
    path: 'C:\\Users\\Omega',
  },
  path: 'C:\\Users\\Omega',
  entries: [
    {
      ...itemsSortPendingListDir.entries[0],
      itemCount: 18,
    },
    {
      ...itemsSortPendingListDir.entries[1],
      itemCount: 6,
    },
    itemsSortPendingListDir.entries[2],
  ],
}

export const itemCountFixtureResponse: VisibleItemCountsRequest = {
  context: {
    paneId: 'left',
    tabId: 'left-tab-1',
    requestId: 1,
    path: 'C:\\Users\\Omega',
  },
  paths: ['C:\\Users\\Omega\\Documents', 'C:\\Users\\Omega\\Designs'],
}

export const itemCountFixtureEvents: IpcEventMap['item-count://state'][] = [
  {
    context: itemCountFixtureResponse.context,
    results: [
      { path: 'C:\\Users\\Omega\\Documents', itemCount: 18 },
      { path: 'C:\\Users\\Omega\\Designs', itemCount: 6 },
    ],
    done: true,
  },
]
