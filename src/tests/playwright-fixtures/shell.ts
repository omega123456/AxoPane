import type { IpcCommandMap } from '@/lib/types/ipc'

export const shellFixtures: Partial<{
  [CommandName in keyof IpcCommandMap]: IpcCommandMap[CommandName]['response']
}> = {
  get_initial_shell: {
    panes: [
      {
        id: 'left',
        title: 'Left pane',
        path: 'C:\\Users\\Omega',
        placeholderHeading: 'Playwright mock shell',
        placeholderBody: 'The browser build uses fixture-backed IPC instead of Tauri runtime APIs.',
      },
      {
        id: 'right',
        title: 'Right pane',
        path: 'D:\\projects',
        placeholderHeading: 'Foundation verified',
        placeholderBody: 'Later phases will replace this shell with the real explorer UI.',
      },
    ],
    treeRoots: [
      { id: 'this-pc', label: 'This PC' },
      { id: 'workspace', label: 'Workspace' },
    ],
  },
}
