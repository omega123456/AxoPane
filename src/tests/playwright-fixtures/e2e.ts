import type { PlatformOs } from '@/lib/keymap'
import type {
  AppConfig,
  IpcCommandMap,
  IpcEventMap,
  ListDirResponse,
  SessionState,
} from '@/lib/types/ipc'
import { TRASH_PATH } from '@/lib/trash'
import { contextMenuFixtures } from './context-menu'
import { deepBreadcrumbListDir, deepBreadcrumbSession } from './breadcrumbs'
import {
  conflictQueueSnapshot,
  deletingQueueProgressEvents,
  deletingQueueSnapshot,
  emptyQueueSnapshot,
  expandedQueueProgressEvents,
  expandedQueueSeedSnapshot,
  expandedQueueSnapshot,
  longPathQueueSnapshot,
  manyPendingQueueSnapshot,
} from './queue'
import { fileTypesListDir } from './file-types'
import { itemsSortPendingListDir, itemsSortPendingSession } from './item-counts'
import { relativeDatesListDir } from './relative-dates'
import { stickyTreeChildrenByPath, stickyTreeListDir, stickyTreeSession } from './tree'
import type { TreeChildrenByPath } from './tree-states'

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
  treeChildrenByPath?: TreeChildrenByPath
  // Forces the app's platform detection for this scenario regardless of the
  // host OS the Playwright run happens on. The native shell-extension menu
  // section only exists on Windows, so the native-menu scenarios pin
  // `'windows'` to stay deterministic on macOS CI/dev machines too.
  platform?: PlatformOs
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
  logLevel: 'info',
  dateFormat: 'ymd',
  showTime: false,
  showSeconds: false,
  relativeDates: false,
  autoFolderSize: true,
  autoExpandActiveQueueToasts: false,
  favourites: [],
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

// Only Name + Modified are shown so the colour-coded relative dates are not
// pushed off-screen by the dual-pane split.
const relativeDatesLightConfig: AppConfig = {
  ...lightConfig,
  relativeDates: true,
  columns: [
    { key: 'name', visible: true },
    { key: 'size', visible: false },
    { key: 'items', visible: false },
    { key: 'type', visible: false },
    { key: 'modified', visible: true },
    { key: 'created', visible: false },
  ],
}

const relativeDatesDarkConfig: AppConfig = {
  ...relativeDatesLightConfig,
  theme: 'dark',
}

const emptyRootListDir: ListDirResponse = {
  path: 'C:\\',
  entries: [],
}

const trashSession: SessionState = {
  activePane: 'left',
  leftPath: TRASH_PATH,
  rightPath: 'D:\\projects',
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
      { path: 'C:\\Users\\Omega', sortKey: 'name', sortDirection: 'asc', filter: '', locked: true },
      { path: 'C:\\', sortKey: 'name', sortDirection: 'asc', filter: '', locked: false },
      { path: 'E:\\', sortKey: 'name', sortDirection: 'asc', filter: '', locked: false },
      { path: 'Z:\\', sortKey: 'name', sortDirection: 'asc', filter: '', locked: false },
    ],
  },
}

function scenarioByTheme(
  commands: CommandMap,
  platform?: PlatformOs,
): {
  light: PlaywrightScenario
  dark: PlaywrightScenario
} {
  return {
    light: {
      commands: {
        load_config: lightConfig,
        ...commands,
      },
      platform,
    },
    dark: {
      commands: {
        load_config: darkConfig,
        ...commands,
      },
      platform,
    },
  }
}

function delayedScenarioByTheme(
  commands: CommandMap,
  delaysMs: DelayMap,
  platform?: PlatformOs,
): { light: PlaywrightScenario; dark: PlaywrightScenario } {
  return {
    light: {
      commands: {
        load_config: lightConfig,
        ...commands,
      },
      delaysMs,
      platform,
    },
    dark: {
      commands: {
        load_config: darkConfig,
        ...commands,
      },
      delaysMs,
      platform,
    },
  }
}

function errorScenarioByTheme(
  commands: CommandMap,
  commandErrors: ErrorMap,
  platform?: PlatformOs,
): { light: PlaywrightScenario; dark: PlaywrightScenario } {
  return {
    light: {
      commands: {
        load_config: lightConfig,
        ...commands,
      },
      commandErrors,
      platform,
    },
    dark: {
      commands: {
        load_config: darkConfig,
        ...commands,
      },
      commandErrors,
      platform,
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
  stickyTree: {
    light: {
      commands: {
        load_config: lightConfig,
        load_session: stickyTreeSession,
        list_dir: stickyTreeListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
      treeChildrenByPath: stickyTreeChildrenByPath,
    },
    dark: {
      commands: {
        load_config: darkConfig,
        load_session: stickyTreeSession,
        list_dir: stickyTreeListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
      treeChildrenByPath: stickyTreeChildrenByPath,
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
  favourites: {
    light: {
      commands: {
        load_config: {
          ...lightConfig,
          favourites: ['C:\\Users\\Omega\\Documents', 'D:\\projects', 'E:\\'],
        },
        load_session: multiTabSession,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: {
          ...darkConfig,
          favourites: ['C:\\Users\\Omega\\Documents', 'D:\\projects', 'E:\\'],
        },
        load_session: multiTabSession,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
  },
  breadcrumbs: {
    light: {
      commands: {
        load_config: {
          ...lightConfig,
          layout: {
            ...lightConfig.layout,
            paneSplit: 0.18,
          },
        },
        load_session: deepBreadcrumbSession,
        list_dir: deepBreadcrumbListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
      platform: 'windows',
    },
    dark: {
      commands: {
        load_config: {
          ...darkConfig,
          layout: {
            ...darkConfig.layout,
            paneSplit: 0.18,
          },
        },
        load_session: deepBreadcrumbSession,
        list_dir: deepBreadcrumbListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
      platform: 'windows',
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
  relativeDates: {
    light: {
      commands: {
        load_config: relativeDatesLightConfig,
        list_dir: relativeDatesListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: relativeDatesDarkConfig,
        list_dir: relativeDatesListDir,
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
          [
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
        ],
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
      },
      events: {
        'size://state': [
          [
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
        ],
      },
    },
  },
  itemsSortPending: {
    light: {
      commands: {
        load_config: lightConfig,
        load_session: itemsSortPendingSession,
        list_dir: itemsSortPendingListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
      delaysMs: {
        sort_active_items: 5_000,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        load_session: itemsSortPendingSession,
        list_dir: itemsSortPendingListDir,
        queue_snapshot: emptyQueueSnapshot,
      },
      delaysMs: {
        sort_active_items: 5_000,
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
  queueManyPending: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: manyPendingQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: manyPendingQueueSnapshot,
      },
    },
  },
  queueDeleting: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: deletingQueueSnapshot,
      },
      events: {
        'queue://progress': deletingQueueProgressEvents,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: deletingQueueSnapshot,
      },
      events: {
        'queue://progress': deletingQueueProgressEvents,
      },
    },
  },
  queueLongPath: {
    light: {
      commands: {
        load_config: lightConfig,
        queue_snapshot: longPathQueueSnapshot,
      },
    },
    dark: {
      commands: {
        load_config: darkConfig,
        queue_snapshot: longPathQueueSnapshot,
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
  rowContextMenu: scenarioByTheme(
    {
      queue_snapshot: emptyQueueSnapshot,
      load_native_menu: contextMenuFixtures.nativeExtras,
    },
    'windows',
  ),
  rowContextMenuLoading: delayedScenarioByTheme(
    {
      queue_snapshot: emptyQueueSnapshot,
      load_native_menu: contextMenuFixtures.nativeExtras,
    },
    {
      load_native_menu: 1_500,
    },
    'windows',
  ),
  rowContextMenuFailure: errorScenarioByTheme(
    {
      queue_snapshot: emptyQueueSnapshot,
      load_native_menu: contextMenuFixtures.emptyNativeExtras,
    },
    {
      load_native_menu: contextMenuFixtures.nativeFailureMessage,
    },
    'windows',
  ),
  defaultAppDialog: scenarioByTheme(
    {
      list_dir: fileTypesListDir,
      queue_snapshot: emptyQueueSnapshot,
    },
    'macos',
  ),
  defaultAppDialogError: scenarioByTheme(
    {
      list_dir: fileTypesListDir,
      queue_snapshot: emptyQueueSnapshot,
      set_default_application: {
        handled: false,
        message: 'default-application-rejected-dynamic-type',
      },
    },
    'macos',
  ),
  trash: scenarioByTheme({
    load_session: trashSession,
    queue_snapshot: emptyQueueSnapshot,
  }),
  // Eject is macOS-only (Windows uses the native shell "Eject"), so pin the
  // removable-drive tree menu to macOS to capture the eject entry deterministically.
  ejectMenu: scenarioByTheme(
    {
      queue_snapshot: emptyQueueSnapshot,
    },
    'macos',
  ),
} satisfies Record<string, { light: PlaywrightScenario; dark: PlaywrightScenario }>
