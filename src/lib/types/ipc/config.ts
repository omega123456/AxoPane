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
  /** Folder-tree sidebar width in pixels (user-draggable). */
  treeWidthPx: number
  /** Favourites list max height in pixels (user-draggable). */
  favouritesHeightPx: number
  /** Fraction of the dual-pane area allotted to the left pane (0..1, user-draggable). */
  paneSplit: number
  /** Per-column widths in CSS pixels. Missing keys fall back to app defaults. */
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
  /** Custom display names for favourites, keyed by the favourite's path. */
  favouriteNames: Record<string, string>
  favouritesCollapsed: boolean
}

export type SessionTab = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  /** Optional so older saved sessions remain distinguishable during migration. */
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
  /** Recently visited folders, most recent first (Search everywhere). */
  recentFolders?: string[]
}

export type SaveConfigRequest = {
  config: AppConfig
}

export type SaveSessionRequest = {
  session: SessionState
}
