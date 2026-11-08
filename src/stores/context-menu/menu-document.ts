import {
  nativeInvocationContextAction,
  noopContextAction,
} from '@/lib/context-menu/context-menu-actions'
import {
  isContextMenuSubmenuRow,
  type ContextMenuDocument,
  type ContextMenuIcon,
  type ContextMenuRowItem,
  type ContextMenuSection,
  type ContextMenuSubmenuRow,
  type ContextMenuSubmenuRowItem,
} from '@/lib/types/context-menu'
import type { NativeMenuIcon, NativeMenuItem } from '@/lib/types/ipc'

export const EMPTY_NATIVE_ROWS: ContextMenuRowItem[] = []

const SUBMENU_PANEL_SUFFIX = '-submenu'
const UNAVAILABLE_NATIVE_ACTION_PREFIX = 'native-unavailable-'

function visibleStripItems(menu: ContextMenuDocument) {
  return menu.topStrip.filter((item) => !item.hidden)
}

function withVisibleChildren(item: ContextMenuRowItem): ContextMenuRowItem {
  if (!isContextMenuSubmenuRow(item)) {
    return item
  }
  return {
    ...item,
    children: {
      ...item.children,
      rows: item.children.rows.filter((child) => !child.hidden),
    },
  }
}

function hasVisibleContent(item: ContextMenuRowItem) {
  return !isContextMenuSubmenuRow(item) || item.children.rows.length > 0
}

function visibleSectionRows(section: ContextMenuSection) {
  return section.rows
    .filter((item) => !item.hidden)
    .map(withVisibleChildren)
    .filter(hasVisibleContent)
}

export function sanitizeMenu(menu: ContextMenuDocument): ContextMenuDocument {
  const isNativeSection = (section: ContextMenuSection) => section.id === menu.nativeSectionId
  return {
    ...menu,
    topStrip: visibleStripItems(menu),
    sections: menu.sections
      .map((section) => ({ ...section, rows: visibleSectionRows(section) }))
      .filter((section) => section.rows.length > 0 || isNativeSection(section)),
  }
}

function hasSection(menu: ContextMenuDocument, sectionId: string) {
  return menu.sections.some((section) => section.id === sectionId)
}

export function injectNativeSection(menu: ContextMenuDocument): ContextMenuDocument {
  const nativeSectionId = menu.nativeSectionId
  if (!menu.nativeRequest) {
    return menu
  }
  if (!nativeSectionId) {
    return menu
  }
  if (hasSection(menu, nativeSectionId)) {
    return menu
  }

  const nativeSection: ContextMenuSection = { id: nativeSectionId, rows: EMPTY_NATIVE_ROWS }
  const insertIndex = Math.max(menu.sections.length - 1, 0)
  const sections = [...menu.sections]
  sections.splice(insertIndex, 0, nativeSection)
  return { ...menu, sections }
}

export function replaceNativeSectionRows(
  menu: ContextMenuDocument,
  rows: ContextMenuRowItem[],
): ContextMenuDocument {
  if (!menu.nativeSectionId) {
    return menu
  }

  return {
    ...menu,
    sections: menu.sections.map((section) =>
      section.id === menu.nativeSectionId ? { ...section, rows } : section,
    ),
  }
}

function mapNativeIcon(icon: NativeMenuIcon | null): ContextMenuIcon | undefined {
  if (!icon) {
    return undefined
  }

  return {
    kind: 'native',
    dataUrl: icon.dataUrl,
    alt: icon.alt ?? undefined,
  }
}

function mapNativeChildItem(item: NativeMenuItem): ContextMenuSubmenuRow | null {
  if (!item.invokeToken) {
    return null
  }

  return {
    id: item.id,
    label: item.label,
    owner: 'native',
    icon: mapNativeIcon(item.icon),
    disabled: !item.enabled,
    danger: item.danger,
    action: nativeInvocationContextAction(item.invokeToken),
  }
}

function usableInvokeToken(item: NativeMenuItem): string | null {
  const hasToken = typeof item.invokeToken === 'string' && item.invokeToken.length > 0
  return hasToken ? item.invokeToken : null
}

function nativeItemBase(item: NativeMenuItem, canInvoke: boolean) {
  const isDeadLeaf = item.children.length === 0 && !canInvoke
  return {
    id: item.id,
    label: item.label,
    owner: 'native' as const,
    icon: mapNativeIcon(item.icon),
    disabled: !item.enabled || isDeadLeaf,
    danger: item.danger,
  }
}

function nativeSubmenuItem(
  item: NativeMenuItem,
  childRows: ContextMenuSubmenuRow[],
): ContextMenuSubmenuRowItem {
  return {
    ...nativeItemBase(item, usableInvokeToken(item) !== null),
    kind: 'submenu',
    children: {
      id: `${item.id}${SUBMENU_PANEL_SUFFIX}`,
      rows: childRows,
    },
  }
}

function unavailableNativeItem(item: NativeMenuItem): ContextMenuRowItem {
  return {
    ...nativeItemBase(item, false),
    disabled: true,
    kind: 'action',
    action: noopContextAction(`${UNAVAILABLE_NATIVE_ACTION_PREFIX}${item.id}`),
  }
}

export function mapNativeItem(item: NativeMenuItem): ContextMenuRowItem {
  const childRows = item.children.map(mapNativeChildItem).filter((child) => child !== null)
  if (childRows.length > 0) {
    return nativeSubmenuItem(item, childRows)
  }

  const invokeToken = usableInvokeToken(item)
  if (invokeToken === null) {
    return unavailableNativeItem(item)
  }

  return {
    ...nativeItemBase(item, true),
    kind: 'action',
    action: nativeInvocationContextAction(invokeToken),
  }
}
