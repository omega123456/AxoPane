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
        expandability: 'nonEmpty',
      },
      {
        name: 'Program Files',
        path: 'C:\\Program Files',
        expandability: 'nonEmpty',
      },
      {
        name: 'Temp',
        path: 'C:\\Temp',
        expandability: 'nonEmpty',
      },
    ],
  },
  'C:\\Users': {
    path: 'C:\\Users',
    children: [
      {
        name: 'Omega',
        path: 'C:\\Users\\Omega',
        expandability: 'nonEmpty',
      },
      {
        name: 'Public',
        path: 'C:\\Users\\Public',
        expandability: 'nonEmpty',
      },
    ],
  },
  'C:\\Users\\Omega': {
    path: 'C:\\Users\\Omega',
    children: [
      {
        name: 'Desktop',
        path: 'C:\\Users\\Omega\\Desktop',
        expandability: 'nonEmpty',
      },
      {
        name: 'Documents',
        path: 'C:\\Users\\Omega\\Documents',
        expandability: 'nonEmpty',
      },
      {
        name: 'Projects',
        path: 'C:\\Users\\Omega\\Projects',
        expandability: 'nonEmpty',
      },
    ],
  },
  'C:\\Users\\Omega\\Projects': {
    path: 'C:\\Users\\Omega\\Projects',
    children: [
      {
        name: 'Client A',
        path: 'C:\\Users\\Omega\\Projects\\Client A',
        expandability: 'nonEmpty',
      },
      {
        name: 'Client B',
        path: 'C:\\Users\\Omega\\Projects\\Client B',
        expandability: 'nonEmpty',
      },
      {
        name: 'Internal',
        path: 'C:\\Users\\Omega\\Projects\\Internal',
        expandability: 'nonEmpty',
      },
    ],
  },
  'C:\\Users\\Omega\\Projects\\Client A': {
    path: 'C:\\Users\\Omega\\Projects\\Client A',
    children: [
      {
        name: 'Briefs',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Briefs',
        expandability: 'nonEmpty',
      },
      {
        name: 'Designs',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Designs',
        expandability: 'nonEmpty',
      },
      {
        name: 'Exports',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Exports',
        expandability: 'nonEmpty',
      },
      {
        name: 'Notes',
        path: 'C:\\Users\\Omega\\Projects\\Client A\\Notes',
        expandability: 'nonEmpty',
      },
    ],
  },
}

// A ten-level active path whose middle levels fold into one "levels hidden"
// row. Each path folder sits among real-looking siblings so the folded
// "other folders" counts match what a user would see.
const DEEP_TREE_LEVELS: { name: string; siblings: string[] }[] = [
  { name: 'Users', siblings: ['PerfLogs', 'Program Files', 'Windows'] },
  { name: 'Omega', siblings: ['Default', 'Public'] },
  { name: 'Projects', siblings: ['Desktop', 'Documents', 'Downloads', 'Music'] },
  { name: 'AxoPane', siblings: ['archive', 'dotfiles', 'homelab'] },
  { name: 'src-tauri', siblings: ['docs', 'e2e', 'src'] },
  { name: 'target', siblings: ['capabilities', 'icons', 'tests'] },
  { name: 'debug', siblings: ['nextest', 'release'] },
  { name: 'build', siblings: ['deps', 'examples', 'incremental'] },
  { name: 'axopane-3f9c1e', siblings: ['libc-9a8b7c', 'serde-445566', 'tauri-778899'] },
  { name: 'out', siblings: [] },
]

const deepTreePaths = DEEP_TREE_LEVELS.reduce<string[]>(
  (paths, level) => [...paths, `${paths[paths.length - 1].replace(/\\$/, '')}\\${level.name}`],
  ['C:\\'],
)

export const deepTreePath = deepTreePaths[deepTreePaths.length - 1]

export const deepTreeSession: SessionState = {
  activePane: 'left',
  leftPath: deepTreePath,
  rightPath: 'D:\\projects',
}

export const deepTreeListDir: ListDirResponse = {
  path: deepTreePath,
  entries: [
    {
      id: 'bindings',
      name: 'bindings.rs',
      path: `${deepTreePath}\\bindings.rs`,
      isDir: false,
      sizeBytes: 18_432,
      itemCount: null,
      typeLabel: 'Rust source',
      modifiedAt: '2026-06-24T10:15:00Z',
      createdAt: '2026-06-12T10:15:00Z',
      attributes: [],
      isHidden: false,
      isSystem: false,
    },
  ],
}

export const deepTreeChildrenByPath: Record<string, ListTreeChildrenResponse> = Object.fromEntries(
  DEEP_TREE_LEVELS.map((level, index) => {
    const parent = deepTreePaths[index]
    const parentPrefix = parent.replace(/\\$/, '')
    const names = [...level.siblings, level.name].sort((left, right) => left.localeCompare(right))
    return [
      parent,
      {
        path: parent,
        children: names.map((name) => ({
          name,
          path: `${parentPrefix}\\${name}`,
          expandability: name === 'out' ? 'empty' : 'nonEmpty',
        })),
      },
    ]
  }),
)
