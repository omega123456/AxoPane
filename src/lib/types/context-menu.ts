import type { ContextMenuAction } from '@/lib/context-menu/context-menu-actions'
import type { LoadNativeMenuRequest, NativeMenuTargetKind } from '@/lib/types/ipc'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

export type ContextMenuTarget =
  | { kind: 'file'; entry: DirectoryEntry }
  | { kind: 'folder'; entry: DirectoryEntry }
  | { kind: 'multi' }
  | { kind: 'empty' }
  | { kind: 'tab'; tabId: string }
  | { kind: 'tree'; path: string }

export type ContextMenuNativeRequest = Omit<LoadNativeMenuRequest, 'requestId'> & {
  targetKind: NativeMenuTargetKind
}

export type ContextMenuAppIconName =
  | 'archive'
  | 'calculate-size'
  | 'close-tab'
  | 'copy'
  | 'cut'
  | 'delete'
  | 'extract'
  | 'new-file'
  | 'new-folder'
  | 'open'
  | 'open-in-new-tab'
  | 'open-in-other-pane'
  | 'open-with'
  | 'paste'
  | 'properties'
  | 'refresh'
  | 'rename'
  | 'select-all'
  | 'share'

export type ContextMenuIcon =
  | {
      kind: 'app'
      name: ContextMenuAppIconName
    }
  | {
      kind: 'native'
      dataUrl: string
      alt?: string
    }

type ContextMenuBaseItem = {
  id: string
  label: string
  owner: 'app' | 'native'
  shortcut?: string
  icon?: ContextMenuIcon
  disabled?: boolean
  danger?: boolean
  hidden?: boolean
  strong?: boolean
}

export type ContextMenuStripItem = ContextMenuBaseItem & {
  action: ContextMenuAction
}

export type ContextMenuActionRow = ContextMenuBaseItem & {
  kind: 'action'
  action: ContextMenuAction
}

export type ContextMenuSubmenuRow = ContextMenuBaseItem & {
  action: ContextMenuAction
}

export type ContextMenuSubmenuPanel = {
  id: string
  rows: ContextMenuSubmenuRow[]
}

export type ContextMenuSubmenuRowItem = ContextMenuBaseItem & {
  kind: 'submenu'
  children: ContextMenuSubmenuPanel
}

export type ContextMenuRowItem = ContextMenuActionRow | ContextMenuSubmenuRowItem

export type ContextMenuSection = {
  id: string
  rows: ContextMenuRowItem[]
}

export type ContextMenuDocument = {
  paneId: PaneId
  x: number
  y: number
  title: string
  chip?: string
  targetId?: string
  topStrip: ContextMenuStripItem[]
  sections: ContextMenuSection[]
  nativeRequest?: ContextMenuNativeRequest | null
  nativeSectionId?: string | null
}

export type ContextMenuContent = Pick<
  ContextMenuDocument,
  'topStrip' | 'sections' | 'nativeRequest' | 'nativeSectionId'
>

export function isContextMenuSubmenuRow(item: ContextMenuRowItem): item is ContextMenuSubmenuRowItem {
  return item.kind === 'submenu'
}
