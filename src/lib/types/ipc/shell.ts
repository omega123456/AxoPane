import type { SortKey } from './listing'

export type ThemePreference = 'system' | 'light' | 'dark'
export type PaneMode = 'dual' | 'single'
export type ZoomLevel = '80' | '90' | '100' | '110' | '120' | '125' | '150'
export type Shortcut = string
export type CommandId =
  | 'open'
  | 'goUp'
  | 'refresh'
  | 'rename'
  | 'delete'
  | 'deletePermanent'
  | 'restore'
  | 'emptyTrash'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'copyToOtherPane'
  | 'moveToOtherPane'
  | 'newFolder'
  | 'newFile'
  | 'calculateSize'
  | 'openInNewTab'
  | 'openInOtherPane'
  | 'selectAll'
  | 'clearFilter'
  | 'showSettings'
  | 'searchEverywhere'

export type ColumnKey = SortKey

export type PaneShell = {
  id: 'left' | 'right'
  title: string
  path: string
  placeholderHeading: string
  placeholderBody: string
}

export type TreeRoot = {
  id: string
  label: string
}

export type InitialShellResponse = {
  panes: PaneShell[]
  treeRoots: TreeRoot[]
}
