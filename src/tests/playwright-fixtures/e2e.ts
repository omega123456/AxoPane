import type {
  AppConfig,
  IpcCommandMap,
  IpcEventMap,
  ListDirResponse,
  SessionState,
} from '@/lib/types/ipc'
import { conflictQueueSnapshot, emptyQueueSnapshot, expandedQueueSnapshot } from './queue'

type CommandMap = Partial<{
  [CommandName in keyof IpcCommandMap]: IpcCommandMap[CommandName]['response']
}>

type ErrorMap = Partial<Record<keyof IpcCommandMap, string>>
type DelayMap = Partial<Record<keyof IpcCommandMap, number>>
type EventMap = Partial<{
  [EventName in keyof IpcEventMap]: IpcEventMap[EventName][]
}>

export type PlaywrightScenario = {
  commands?: CommandMap
  commandErrors?: ErrorMap
  delaysMs?: DelayMap
  events?: EventMap
}

const rootSession: SessionState = {
  activePane: 'left',
  leftPath: 'C:\\',
  rightPath: 'D:\\projects',
}

const lightConfig: AppConfig = {
  theme: 'light',
  showHiddenFiles: false,
  dismissedEverythingBanner: false,
  keybindings: {},
  columns: [
    { key: 'name', visible: true },
    { key: 'size', visible: true },
    { key: 'items', visible: true },
    { key: 'type', visible: true },
    { key: 'modified', visible: true },
    { key: 'created', visible: false },
  ],
  layout: {
    detailsVisible: false,
    treeWidth: 'default',
    defaultPaneMode: 'dual',
    restoreSession: true,
  },
}

const darkConfig: AppConfig = {
  ...lightConfig,
  theme: 'dark',
}

const emptyRootListDir: ListDirResponse = {
  path: 'C:\\',
  entries: [],
}

export const screenshotScenarios = {
  browsing: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
  },
  loading: {
    light: {
      commands: {
        load_config: lightConfig,
      },
      delaysMs: {
        list_dir: 1_500,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
      },
      delaysMs: {
        list_dir: 1_500,
      },
    },
  },
  empty: {
    light: {
      commands: {
        load_config: lightConfig,
        load_session: rootSession,
        list_dir: emptyRootListDir,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        load_session: rootSession,
        list_dir: emptyRootListDir,
      },
    },
  },
  error: {
    light: {
      commands: {
        load_config: lightConfig,
      },
      commandErrors: {
        list_dir: 'Directory refresh failed: device timeout.',
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
      },
      commandErrors: {
        list_dir: 'Directory refresh failed: device timeout.',
      },
    },
  },
  permission: {
    light: {
      commands: {
        load_config: lightConfig,
      },
      commandErrors: {
        list_dir: 'Access is denied.',
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
      },
      commandErrors: {
        list_dir: 'Access is denied.',
      },
    },
  },
  sizes: {
    light: {
      commands: {
        load_config: lightConfig,
      },
      events: {
        'size://state': [
          {
            path: 'C:\\Users\\Omega\\Documents',
            state: 'calculating',
            source: 'manual',
            sizeBytes: null,
          },
          {
            path: 'C:\\Users\\Omega\\Media',
            state: 'ready',
            source: 'everything',
            sizeBytes: 987_654_321,
          },
        ],
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
      },
      events: {
        'size://state': [
          {
            path: 'C:\\Users\\Omega\\Documents',
            state: 'calculating',
            source: 'manual',
            sizeBytes: null,
          },
          {
            path: 'C:\\Users\\Omega\\Media',
            state: 'ready',
            source: 'everything',
            sizeBytes: 987_654_321,
          },
        ],
      },
    },
  },
  queueCollapsed: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: expandedQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: expandedQueueSnapshot,
      },
    },
  },
  queueExpanded: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: expandedQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: expandedQueueSnapshot,
      },
    },
  },
  conflict: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: conflictQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: conflictQueueSnapshot,
      },
    },
  },
} satisfies Record<string, { light: PlaywrightScenario; dark: PlaywrightScenario }>
