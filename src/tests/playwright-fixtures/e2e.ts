import type {
  AppConfig,
  IpcCommandMap,
  IpcEventMap,
  ListDirResponse,
  SessionState,
} from '@/lib/types/ipc'
import { contextMenuFixtures } from './context-menu'
import {
  conflictQueueSnapshot,
  emptyQueueSnapshot,
  expandedQueueProgressEvents,
  expandedQueueSeedSnapshot,
  expandedQueueSnapshot,
} from './queue'
import { fileTypesListDir } from './file-types'

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
  updateCheckInterval: '1d',
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
    treeWidthPx: 204,
    paneSplit: 0.5,
    columnWidths: {
      name: 320,
      size: 96,
      items: 72,
      type: 136,
      modified: 128,
      created: 128,
    },
    defaultPaneMode: 'dual',
    restoreSession: true,
    zoom: '100',
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

// A left pane with several tabs whose locations exercise every LocationIcon
// branch: an active folder tab plus inactive fixed / removable / network roots.
const multiTabSession: SessionState = {
  activePane: 'left',
  leftPath: 'C:\\Users\\Omega',
  rightPath: 'D:\\projects',
  left: {
    activeTabIndex: 0,
    tabs: [
      { path: 'C:\\Users\\Omega', sortKey: 'name', sortDirection: 'asc', filter: '' },
      { path: 'C:\\', sortKey: 'name', sortDirection: 'asc', filter: '' },
      { path: 'E:\\', sortKey: 'name', sortDirection: 'asc', filter: '' },
      { path: 'Z:\\', sortKey: 'name', sortDirection: 'asc', filter: '' },
    ],
  },
}

function scenarioByTheme(commands: CommandMap): {
  light: PlaywrightScenario
  dark: PlaywrightScenario
} {
  return {
    light: {
      commands: {
        load_config: lightConfig,
        ...commands,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        ...commands,
      },
    },
  }
}

function delayedScenarioByTheme(
  commands: CommandMap,
  delaysMs: DelayMap,
): { light: PlaywrightScenario; dark: PlaywrightScenario } {
  return {
    light: {
      commands: {
        load_config: lightConfig,
        ...commands,
      },
      delaysMs,
    },
    dark: {
      commands: {
        load_config: darkConfig,
        ...commands,
      },
      delaysMs,
    },
  }
}

function errorScenarioByTheme(
  commands: CommandMap,
  commandErrors: ErrorMap,
): { light: PlaywrightScenario; dark: PlaywrightScenario } {
  return {
    light: {
      commands: {
        load_config: lightConfig,
        ...commands,
      },
      commandErrors,
    },
    dark: {
      commands: {
        load_config: darkConfig,
        ...commands,
      },
      commandErrors,
    },
  }
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
  tabs: {
    light: {
      commands: {
        load_config: lightConfig,
        load_session: multiTabSession,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        load_session: multiTabSession,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
  },
  fileTypes: {
    light: {
      commands: {
        load_config: lightConfig,
        list_dir: fileTypesListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        list_dir: fileTypesListDir,
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
        queue_snapshot: expandedQueueSeedSnapshot,
      },
      events: {
        'queue://progress': expandedQueueProgressEvents,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: expandedQueueSeedSnapshot,
      },
      events: {
        'queue://progress': expandedQueueProgressEvents,
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
  paneContextMenu: scenarioByTheme({
    queue_snapshot: emptyQueueSnapshot,
    load_native_menu: contextMenuFixtures.emptyNativeExtras,
  }),
  rowContextMenu: scenarioByTheme({
    queue_snapshot: emptyQueueSnapshot,
    load_native_menu: contextMenuFixtures.nativeExtras,
  }),
  rowContextMenuLoading: delayedScenarioByTheme(
    {
      queue_snapshot: emptyQueueSnapshot,
      load_native_menu: contextMenuFixtures.nativeExtras,
    },
    {
      load_native_menu: 1_500,
    },
  ),
  rowContextMenuFailure: errorScenarioByTheme(
    {
      queue_snapshot: emptyQueueSnapshot,
      load_native_menu: contextMenuFixtures.emptyNativeExtras,
    },
    {
      load_native_menu: contextMenuFixtures.nativeFailureMessage,
    },
  ),
} satisfies Record<string, { light: PlaywrightScenario; dark: PlaywrightScenario }>
