import type { ContextMenuAction } from '@/lib/context-menu/context-menu-actions'
import {
  isContextMenuSubmenuRow,
  type ContextMenuDocument,
  type ContextMenuRowItem,
  type ContextMenuSection,
  type ContextMenuStripItem,
  type ContextMenuSubmenuRow,
  type ContextMenuSubmenuRowItem,
} from '@/lib/types/context-menu'
import { wrapIndex } from '@/lib/numbers'
import type { PaneId } from '@/types/pane'

type FocusableItem = ContextMenuStripItem | ContextMenuRowItem | ContextMenuSubmenuRow

type FocusableLookup = {
  item: FocusableItem
  parentId: string | null
}

export type FocusDirection = 1 | -1
export type FocusEdge = 'start' | 'end'

export type MenuFocus = {
  activeItemId: string | null
  openSubmenuId: string | null
  activeSubmenuItemId: string | null
}

export type MenuFocusState = MenuFocus & { menu: ContextMenuDocument | null }

export type MenuActivation =
  | { kind: 'dispatch'; paneId: PaneId; action: ContextMenuAction }
  | { kind: 'focus'; focus: MenuFocus }

const NO_SUBMENU = { openSubmenuId: null, activeSubmenuItemId: null }

function topLevelLookup(item: FocusableItem): FocusableLookup {
  return { item, parentId: null }
}

function submenuChildLookups(row: ContextMenuRowItem): FocusableLookup[] {
  if (!isContextMenuSubmenuRow(row)) {
    return []
  }
  return row.children.rows.map((child) => ({ item: child, parentId: row.id }))
}

function sectionLookups(section: ContextMenuSection): FocusableLookup[] {
  return section.rows.flatMap((row) => [topLevelLookup(row), ...submenuChildLookups(row)])
}

function findItem(menu: ContextMenuDocument, itemId: string): FocusableLookup | null {
  const lookups = [...menu.topStrip.map(topLevelLookup), ...menu.sections.flatMap(sectionLookups)]
  return lookups.find((lookup) => lookup.item.id === itemId) ?? null
}

function getTopLevelItems(menu: ContextMenuDocument): FocusableItem[] {
  return [...menu.topStrip, ...menu.sections.flatMap((section) => section.rows)]
}

function getSubmenuChildren(menu: ContextMenuDocument, parentId: string) {
  const parent = menu.sections
    .flatMap((section) => section.rows)
    .find((row) => isContextMenuSubmenuRow(row) && row.id === parentId)
  return parent && isContextMenuSubmenuRow(parent) ? parent.children.rows : []
}

function isSubmenuParent(item: FocusableItem): item is ContextMenuSubmenuRowItem {
  return 'kind' in item && item.kind === 'submenu'
}

function getEnabledItems(items: FocusableItem[]) {
  return items.filter((item) => !item.disabled)
}

function firstEnabledId(items: FocusableItem[]) {
  return getEnabledItems(items)[0]?.id ?? null
}

function lastEnabledId(items: FocusableItem[]) {
  return getEnabledItems(items).at(-1)?.id ?? null
}

const edgeEnabledId: Record<FocusEdge, (items: FocusableItem[]) => string | null> = {
  start: firstEnabledId,
  end: lastEnabledId,
}

function entryIndexForDirection(direction: FocusDirection, length: number) {
  return direction === 1 ? 0 : length - 1
}

function moveWithin(
  items: FocusableItem[],
  activeItemId: string | null,
  direction: FocusDirection,
) {
  const enabled = getEnabledItems(items)
  if (enabled.length === 0) {
    return activeItemId
  }

  const currentIndex = enabled.findIndex((item) => item.id === activeItemId)
  if (currentIndex < 0) {
    return enabled[entryIndexForDirection(direction, enabled.length)]?.id ?? activeItemId
  }
  const nextIndex = wrapIndex(currentIndex + direction, enabled.length)
  return enabled[nextIndex]?.id ?? activeItemId
}

function activeParentId(menu: ContextMenuDocument, activeItemId: string | null) {
  const lookup = activeItemId ? findItem(menu, activeItemId) : null
  return lookup?.parentId ?? null
}

function focusSubmenuItem(parentId: string, activeSubmenuItemId: string | null): MenuFocus {
  return { activeItemId: activeSubmenuItemId, openSubmenuId: parentId, activeSubmenuItemId }
}

function submenuParentOf(menu: ContextMenuDocument, itemId: string | null) {
  const item = itemId ? findItem(menu, itemId)?.item : undefined
  if (!item) {
    return null
  }
  return isSubmenuParent(item) ? item : null
}

function focusTopLevelItem(menu: ContextMenuDocument, activeItemId: string | null): MenuFocus {
  const submenu = submenuParentOf(menu, activeItemId)
  if (!submenu) {
    return { activeItemId, ...NO_SUBMENU }
  }
  return {
    activeItemId,
    openSubmenuId: activeItemId,
    activeSubmenuItemId: firstEnabledId(submenu.children.rows),
  }
}

function focusOpenedSubmenu(
  submenu: ContextMenuSubmenuRowItem,
  activeSubmenuItemId: string | null,
): MenuFocus {
  return {
    activeItemId: activeSubmenuItemId ?? submenu.id,
    openSubmenuId: submenu.id,
    activeSubmenuItemId,
  }
}

function openableSubmenu(menu: ContextMenuDocument, itemId: string) {
  const lookup = findItem(menu, itemId)
  if (!lookup) {
    return null
  }
  if (lookup.parentId) {
    return null
  }
  if (!isSubmenuParent(lookup.item)) {
    return null
  }
  return lookup.item.disabled ? null : lookup.item
}

function submenuChildToFocus(state: MenuFocus, submenu: ContextMenuSubmenuRowItem) {
  const firstChildId = firstEnabledId(submenu.children.rows)
  if (state.openSubmenuId !== submenu.id) {
    return firstChildId
  }
  return state.activeSubmenuItemId || firstChildId
}

function enabledLookup(menu: ContextMenuDocument, itemId: string) {
  const lookup = findItem(menu, itemId)
  if (!lookup || lookup.item.disabled) {
    return null
  }
  return lookup
}

export const CLOSED_FOCUS: MenuFocus = {
  activeItemId: null,
  ...NO_SUBMENU,
}

export function initialFocus(menu: ContextMenuDocument): MenuFocus {
  return { ...CLOSED_FOCUS, activeItemId: firstEnabledId(getTopLevelItems(menu)) }
}

export function hoveredFocus(state: MenuFocusState, itemId: string): MenuFocus | null {
  if (!state.menu) {
    return null
  }
  const lookup = enabledLookup(state.menu, itemId)
  if (!lookup) {
    return null
  }
  if (lookup.parentId) {
    return focusSubmenuItem(lookup.parentId, itemId)
  }
  return focusTopLevelItem(state.menu, itemId)
}

export function movedFocus(state: MenuFocusState, direction: FocusDirection): MenuFocus | null {
  if (!state.menu) {
    return null
  }

  const parentId = activeParentId(state.menu, state.activeItemId)
  if (parentId) {
    const children = getSubmenuChildren(state.menu, parentId)
    return focusSubmenuItem(parentId, moveWithin(children, state.activeItemId, direction))
  }

  const topLevelItems = getTopLevelItems(state.menu)
  return focusTopLevelItem(state.menu, moveWithin(topLevelItems, state.activeItemId, direction))
}

export function edgeFocus(state: MenuFocusState, edge: FocusEdge): MenuFocus | null {
  if (!state.menu) {
    return null
  }

  const parentId = activeParentId(state.menu, state.activeItemId)
  if (parentId) {
    const children = getSubmenuChildren(state.menu, parentId)
    return focusSubmenuItem(parentId, edgeEnabledId[edge](children))
  }

  return { activeItemId: edgeEnabledId[edge](getTopLevelItems(state.menu)), ...NO_SUBMENU }
}

export function openedSubmenuFocus(state: MenuFocusState, itemId?: string): MenuFocus | null {
  const targetId = itemId ?? state.activeItemId
  if (!state.menu || !targetId) {
    return null
  }

  const submenu = openableSubmenu(state.menu, targetId)
  if (!submenu) {
    return null
  }
  return focusOpenedSubmenu(submenu, submenuChildToFocus(state, submenu))
}

export function closedSubmenuFocus(state: MenuFocusState): MenuFocus | null {
  if (!state.menu || !state.openSubmenuId) {
    return null
  }
  return { activeItemId: state.openSubmenuId, ...NO_SUBMENU }
}

export function activationFor(menu: ContextMenuDocument, itemId: string): MenuActivation | null {
  const item = enabledLookup(menu, itemId)?.item
  if (!item) {
    return null
  }
  if (isSubmenuParent(item)) {
    const firstChildId = firstEnabledId(item.children.rows)
    return { kind: 'focus', focus: focusOpenedSubmenu(item, firstChildId) }
  }
  return { kind: 'dispatch', paneId: menu.paneId, action: item.action }
}
