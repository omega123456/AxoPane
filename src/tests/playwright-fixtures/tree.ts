import type { ListDirResponse, ListTreeChildrenResponse, SessionState } from '@/lib/types/ipc'

export const stickyTreeSession: SessionState = {
  activePane: 'left',
  leftPath: 'C:\\Users\\Omega\\Projects\\Client A\\Designs',
  rightPath: 'D:\\projects',
}

export const stickyTreeListDir: ListDirResponse = {
  path: 'C:\\Users\\Omega\\Projects\\Client A\\Designs',
  entries: [
    {
      id: 'wireframes',
      name: 'Wireframes',
      path: 'C:\\Users\\Omega\\Projects\\Client A\\Designs\\Wireframes',
      isDir: true,
      sizeBytes: null,
      itemCount: 14,
      typeLabel: 'Folder',
      modifiedAt: '2026-06-22T10:15:00Z',
      createdAt: '2026-06-10T10:15:00Z',
      attributes: [],
      isHidden: false,
      isSystem: false,
    },
    {
      id: 'handoff',
      name: 'Handoff.pdf',
      path: 'C:\\Users\\Omega\\Projects\\Client A\\Designs\\Handoff.pdf',
      isDir: false,
      sizeBytes: 409_600,
      itemCount: null,
      typeLabel: 'PDF file',
      modifiedAt: '2026-06-24T10:15:00Z',
      createdAt: '2026-06-12T10:15:00Z',
      attributes: [],
      isHidden: false,
      isSystem: false,
    },
  ],
}

export const stickyTreeChildrenByPath: Record<string, ListTreeChildrenResponse> = {
  'C:\\': {
    path: 'C:\\',
    children: [
      {
        name: 'Users',
        path: 'C:\\Users',
        hasChildren: true,
      },
      {
        name: 'Program Files',
        path: 'C:\\Program Files',
        hasChildren: true,
      },
      {
        name: 'Temp',
        path: 'C:\\Temp',
        hasChildren: true,
      },
    ],
  },
  'C:\\Users': {
    path: 'C:\\Users',
    children: [
      {
        name: 'Omega',
        path: 'C:\\Users\\Omega',
        hasChildren: true,
      },
      {
        name: 'Public',
        path: 'C:\\Users\\Public',
        hasChildren: true,
      },
    ],
  },
  'C:\\Users\\Omega': {
    path: 'C:\\Users\\Omega',
    children: [
      {
        name: 'Desktop',
        path: 'C:\\Users\\Omega\\Desktop',
        hasChildren: true,
      },
      {
        name: 'Documents',
        path: 'C:\\Users\\Omega\\Documents',
        hasChildren: true,
      },
      {
        name: 'Projects',
        path: 'C:\\Users\\Omega\\Projects',
        hasChildren: true,
      },
    ],
  },
  'C:\\Users\\Omega\\Projects': {
    path: 'C:\\Users\\Omega\\Projects',
    children: [
      {
        name: 'Client A',
        path: 'C:\\Users\\Omega\\Projects\\Client A',
        hasChildren: true,
      },
      {
        name: 'Client B',
        path: 'C:\\Users\\Omega\\Projects\\Client B',
        hasChildren: true,
      },
      {
        name: 'Internal',
        path: 'C:\\Users\\Omega\\Projects\\Internal',
        hasChildren: true,
      },
    ],
  },
  'C:\\Users\\Omega\\Projects\\Client A': {
    path: 'C:\\Users\\Omega\\Projects\\Client A',
    children: [
      {
        name: 'Briefs',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Briefs',
        hasChildren: true,
      },
      {
        name: 'Designs',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Designs',
        hasChildren: true,
      },
      {
        name: 'Exports',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Exports',
        hasChildren: true,
      },
      {
        name: 'Notes',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Notes',
        hasChildren: true,
      },
    ],
  },
}
