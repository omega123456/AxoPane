import type { SortDirection, SortKey } from './listing'
import type { ColumnKey, CommandId, PaneMode, Shortcut, ThemePreference, ZoomLevel } from './shell'
import type { DateFormat } from '@/lib/date-format'
import type { PaneViewMode } from '@/lib/pane-view'
import type { UpdateInterval } from '@/lib/update-intervals'

export type ColumnConfig = {
  key: ColumnKey
  visible: boolean
}

export type ColumnWidths = Partial<Record<ColumnKey, number>>

export type LayoutConfig = {
  detailsVisible: boolean
  treeWidthPx: number
  favouritesHeightPx: number
  paneSplit: number
  columnWidths: ColumnWidths
  defaultPaneMode: PaneMode
  defaultViewMode: PaneViewMode
  restoreSession: boolean
  zoom: ZoomLevel
}

export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

export type LogDisplayFilter = 'all' | LogLevel

export type LogEntry = {
  id: number
  timestamp: string
  level: string
  target: string
  message: string
}

export type AppConfig = {
  theme: ThemePreference
  showHiddenFiles: boolean
  dismissedEverythingBanner: boolean
  keybindings: Partial<Record<CommandId, Shortcut[]>>
  columns: ColumnConfig[]
  layout: LayoutConfig
  updateCheckInterval: UpdateInterval
  logLevel: LogLevel
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
  relativeDates: boolean
  autoFolderSize: boolean
  autoExpandActiveQueueToasts: boolean
  favourites: string[]
  favouriteNames: Record<string, string>
  favouritesCollapsed: boolean
  indexExclusions: string[]
}

export type SessionTab = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  viewMode?: string | null
  locked?: boolean
}

export type SessionPane = {
  activeTabIndex: number
  tabs: SessionTab[]
}

export type SessionState = {
  activePane: 'left' | 'right'
  leftPath: string
  rightPath: string
  left?: SessionPane | null
  right?: SessionPane | null
  recentFolders?: string[]
}

export type SaveConfigRequest = {
  config: AppConfig
}

export type SaveSessionRequest = {
  session: SessionState
}
