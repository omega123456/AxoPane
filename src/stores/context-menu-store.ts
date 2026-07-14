import { create } from 'zustand'
import {
  nativeInvocationContextAction,
  noopContextAction,
} from '@/lib/context-menu/context-menu-actions'
import { dispatchContextMenuAction } from '@/lib/context-menu/context-menu-actions'
import {
  isContextMenuSubmenuRow,
  type ContextMenuDocument,
  type ContextMenuIcon,
  type ContextMenuNativeRequest,
  type ContextMenuRowItem,
  type ContextMenuSection,
  type ContextMenuStripItem,
  type ContextMenuSubmenuRowItem,
  type ContextMenuSubmenuRow,
} from '@/lib/types/context-menu'
import type { NativeMenuIcon, NativeMenuItem } from '@/lib/types/ipc'

type FocusableItem = ContextMenuStripItem | ContextMenuRowItem | ContextMenuSubmenuRow

type FocusableLookup = {
  item: FocusableItem
  parentId: string | null
}

type Store = {
  menu: ContextMenuDocument | null
  activeItemId: string | null
  openSubmenuId: string | null
  activeSubmenuItemId: string | null
  nativeRequest: ContextMenuNativeRequest | null
  nativeRequestId: string | null
  nativeLoading: boolean
  nativeFailed: boolean
  nativePlaceholderVisible: boolean
  nativeSectionLocked: boolean
  openMenu: (menu: ContextMenuDocument) => void
  closeMenu: () => void
  beginNativeLoad: () => { requestId: string; request: ContextMenuNativeRequest } | null
  resolveNativeLoad: (requestId: string, items: NativeMenuItem[]) => void
  failNativeLoad: (requestId: string) => void
  hoverItem: (itemId: string) => void
  moveActive: (direction: 1 | -1) => void
  moveActiveToEdge: (edge: 'start' | 'end') => void
  openSubmenu: (itemId?: string) => void
  activateCurrent: () => void
  activateItem: (itemId: string) => void
  closeSubmenu: () => void
}

const EMPTY_NATIVE_ROWS: ContextMenuRowItem[] = []
let nativeRequestSequence = 0
let nativePlaceholderTimer: ReturnType<typeof setTimeout> | null = null

const CLOSED_MENU_STATE = {
  menu: null,
  activeItemId: null,
  openSubmenuId: null,
  activeSubmenuItemId: null,
  nativeRequest: null,
  nativeRequestId: null,
  nativeLoading: false,
  nativeFailed: false,
  nativePlaceholderVisible: false,
  nativeSectionLocked: false,
} satisfies Pick<
  Store,
  | 'menu'
  | 'activeItemId'
  | 'openSubmenuId'
  | 'activeSubmenuItemId'
  | 'nativeRequest'
  | 'nativeRequestId'
  | 'nativeLoading'
  | 'nativeFailed'
  | 'nativePlaceholderVisible'
  | 'nativeSectionLocked'
>

function clearNativePlaceholderTimer() {
  if (nativePlaceholderTimer === null) {
    return
  }

  clearTimeout(nativePlaceholderTimer)
  nativePlaceholderTimer = null
}

function visibleStripItems(menu: ContextMenuDocument) {
  return menu.topStrip.filter((item) => !item.hidden)
}

function visibleSectionRows(section: ContextMenuSection) {
  return section.rows
    .filter((item) => !item.hidden)
    .map((item) =>
      isContextMenuSubmenuRow(item)
        ? {
            ...item,
            children: {
              ...item.children,
              rows: item.children.rows.filter((child) => !child.hidden),
            },
          }
        : item,
    )
    .filter((item) => !isContextMenuSubmenuRow(item) || item.children.rows.length > 0)
}

function sanitizeMenu(menu: ContextMenuDocument): ContextMenuDocument {
  return {
    ...menu,
    topStrip: visibleStripItems(menu),
    sections: menu.sections
      .map((section) => ({
        ...section,
        rows: visibleSectionRows(section),
      }))
      .filter((section) => section.rows.length > 0 || section.id === menu.nativeSectionId),
  }
}

function injectNativeSection(menu: ContextMenuDocument): ContextMenuDocument {
  if (!menu.nativeRequest || !menu.nativeSectionId) {
    return menu
  }

  const existingIndex = menu.sections.findIndex((section) => section.id === menu.nativeSectionId)
  if (existingIndex >= 0) {
    return menu
  }

  const nativeSection: ContextMenuSection = { id: menu.nativeSectionId, rows: EMPTY_NATIVE_ROWS }
  const insertIndex = Math.max(menu.sections.length - 1, 0)
  const sections = [...menu.sections]
  sections.splice(insertIndex, 0, nativeSection)

  return {
    ...menu,
    sections,
  }
}

function getTopLevelItems(menu: ContextMenuDocument): FocusableItem[] {
  return [...menu.topStrip, ...menu.sections.flatMap((section) => section.rows)]
}

function getEnabledItems(items: FocusableItem[]) {
  return items.filter((item) => !item.disabled)
}

function getSubmenuChildren(menu: ContextMenuDocument, parentId: string | null) {
  if (!parentId) {
    return []
  }

  for (const section of menu.sections) {
    for (const item of section.rows) {
      if (isContextMenuSubmenuRow(item) && item.id === parentId) {
        return item.children.rows
      }
    }
  }

  return []
}

function findItem(menu: ContextMenuDocument, itemId: string): FocusableLookup | null {
  for (const item of menu.topStrip) {
    if (item.id === itemId) {
      return { item, parentId: null }
    }
  }

  for (const section of menu.sections) {
    for (const item of section.rows) {
      if (item.id === itemId) {
        return { item, parentId: null }
      }
      if (isContextMenuSubmenuRow(item)) {
        for (const child of item.children.rows) {
          if (child.id === itemId) {
            return { item: child, parentId: item.id }
          }
        }
      }
    }
  }

  return null
}

function isSubmenuParent(item: FocusableItem): item is ContextMenuSubmenuRowItem {
  return 'kind' in item && item.kind === 'submenu'
}

function isSubmenuChildLookup(
  lookup: FocusableLookup,
): lookup is { item: ContextMenuSubmenuRow; parentId: string } {
  return lookup.parentId !== null
}

function firstEnabledId(items: FocusableItem[]) {
  return getEnabledItems(items)[0]?.id ?? null
}

function lastEnabledId(items: FocusableItem[]) {
  const enabled = getEnabledItems(items)
  return enabled[enabled.length - 1]?.id ?? null
}

function moveWithin(items: FocusableItem[], activeItemId: string | null, direction: 1 | -1) {
  const enabled = getEnabledItems(items)
  if (enabled.length === 0) {
    return activeItemId
  }

  const currentIndex = enabled.findIndex((item) => item.id === activeItemId)
  const nextIndex =
    currentIndex < 0
      ? direction === 1
        ? 0
        : enabled.length - 1
      : (currentIndex + direction + enabled.length) % enabled.length

  return enabled[nextIndex]?.id ?? activeItemId
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

function mapNativeItem(item: NativeMenuItem): ContextMenuRowItem | null {
  const childRows = item.children.map(mapNativeChildItem).filter((child) => child !== null)
  const invokeToken =
    typeof item.invokeToken === 'string' && item.invokeToken.length > 0 ? item.invokeToken : null
  const canInvoke = invokeToken !== null
  const base = {
    id: item.id,
    label: item.label,
    owner: 'native' as const,
    icon: mapNativeIcon(item.icon),
    disabled: !item.enabled || (item.children.length === 0 && !canInvoke),
    danger: item.danger,
  }

  if (childRows.length > 0) {
    const submenu: ContextMenuSubmenuRowItem = {
      ...base,
      kind: 'submenu',
      children: {
        id: `${item.id}-submenu`,
        rows: childRows,
      },
    }
    return submenu
  }

  if (!canInvoke) {
    return {
      ...base,
      disabled: true,
      kind: 'action',
      action: noopContextAction(`native-unavailable-${item.id}`),
    }
  }

  return {
    ...base,
    kind: 'action',
    action: nativeInvocationContextAction(invokeToken),
  }
}

function replaceNativeSectionRows(
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

export const useContextMenuStore = create<Store>((set, get) => ({
  menu: null,
  activeItemId: null,
  openSubmenuId: null,
  activeSubmenuItemId: null,
  nativeRequest: null,
  nativeRequestId: null,
  nativeLoading: false,
  nativeFailed: false,
  nativePlaceholderVisible: false,
  nativeSectionLocked: false,
  openMenu: (menu) => {
    clearNativePlaceholderTimer()
    const sanitized = sanitizeMenu(injectNativeSection(menu))
    set({
      ...CLOSED_MENU_STATE,
      menu: sanitized,
      activeItemId: firstEnabledId(getTopLevelItems(sanitized)),
      nativeRequest: sanitized.nativeRequest ?? null,
    })
  },
  closeMenu: () => {
    clearNativePlaceholderTimer()
    set(CLOSED_MENU_STATE)
  },
  beginNativeLoad: () => {
    const state = get()
    if (!state.menu || !state.nativeRequest || state.nativeRequestId) {
      return null
    }

    const requestId = `native-request-${++nativeRequestSequence}`
    clearNativePlaceholderTimer()
    set({
      nativeRequestId: requestId,
      nativeLoading: true,
      nativeFailed: false,
      nativePlaceholderVisible: false,
      nativeSectionLocked: false,
    })
    nativePlaceholderTimer = setTimeout(() => {
      const current = useContextMenuStore.getState()
      if (current.nativeRequestId === requestId && current.nativeLoading) {
        useContextMenuStore.setState({
          nativePlaceholderVisible: true,
          nativeSectionLocked: true,
        })
      }
    }, 1000)
    return { requestId, request: state.nativeRequest }
  },
  resolveNativeLoad: (requestId, items) =>
    set((state) => {
      if (!state.menu || state.nativeRequestId !== requestId) {
        return state
      }

      clearNativePlaceholderTimer()
      const rows = items.map(mapNativeItem).filter((item) => item !== null)

      return {
        menu: sanitizeMenu(replaceNativeSectionRows(state.menu, rows)),
        nativeRequest: null,
        nativeRequestId: null,
        nativeLoading: false,
        nativeFailed: false,
        nativePlaceholderVisible: false,
        nativeSectionLocked: state.nativeSectionLocked || rows.length > 0,
      }
    }),
  failNativeLoad: (requestId) =>
    set((state) => {
      if (!state.menu || state.nativeRequestId !== requestId) {
        return state
      }

      clearNativePlaceholderTimer()
      return {
        menu: sanitizeMenu(replaceNativeSectionRows(state.menu, EMPTY_NATIVE_ROWS)),
        nativeRequest: null,
        nativeRequestId: null,
        nativeLoading: false,
        nativeFailed: true,
        nativePlaceholderVisible: false,
        nativeSectionLocked: state.nativeSectionLocked,
      }
    }),
  hoverItem: (itemId) => {
    const menu = get().menu
    if (!menu) {
      return
    }

    const lookup = findItem(menu, itemId)
    if (!lookup || lookup.item.disabled) {
      return
    }

    if (lookup.parentId) {
      set({
        activeItemId: itemId,
        openSubmenuId: lookup.parentId,
        activeSubmenuItemId: itemId,
      })
      return
    }

    if ('kind' in lookup.item && lookup.item.kind === 'submenu') {
      const childId = firstEnabledId(lookup.item.children.rows)
      set({
        activeItemId: itemId,
        openSubmenuId: itemId,
        activeSubmenuItemId: childId,
      })
      return
    }

    set({ activeItemId: itemId, openSubmenuId: null, activeSubmenuItemId: null })
  },
  moveActive: (direction) =>
    set((state) => {
      if (!state.menu) {
        return state
      }

      const lookup = state.activeItemId ? findItem(state.menu, state.activeItemId) : null
      if (lookup?.parentId) {
        const activeSubmenuItemId = moveWithin(
          getSubmenuChildren(state.menu, lookup.parentId),
          state.activeItemId,
          direction,
        )
        return {
          activeItemId: activeSubmenuItemId,
          openSubmenuId: lookup.parentId,
          activeSubmenuItemId,
        }
      }

      const activeItemId = moveWithin(getTopLevelItems(state.menu), state.activeItemId, direction)
      const nextItem = activeItemId ? findItem(state.menu, activeItemId)?.item : null
      return {
        activeItemId,
        openSubmenuId: activeItemId && nextItem && isSubmenuParent(nextItem) ? activeItemId : null,
        activeSubmenuItemId:
          activeItemId && nextItem && isSubmenuParent(nextItem)
            ? firstEnabledId(nextItem.children.rows)
            : null,
      }
    }),
  moveActiveToEdge: (edge) =>
    set((state) => {
      if (!state.menu) {
        return state
      }

      const lookup = state.activeItemId ? findItem(state.menu, state.activeItemId) : null
      if (lookup?.parentId) {
        const children = getSubmenuChildren(state.menu, lookup.parentId)
        const activeSubmenuItemId =
          edge === 'start' ? firstEnabledId(children) : lastEnabledId(children)
        return {
          activeItemId: activeSubmenuItemId,
          openSubmenuId: lookup.parentId,
          activeSubmenuItemId,
        }
      }

      return {
        activeItemId:
          edge === 'start'
            ? firstEnabledId(getTopLevelItems(state.menu))
            : lastEnabledId(getTopLevelItems(state.menu)),
        openSubmenuId: null,
        activeSubmenuItemId: null,
      }
    }),
  openSubmenu: (itemId) =>
    set((state) => {
      if (!state.menu) {
        return state
      }

      const targetId = itemId ?? state.activeItemId
      if (!targetId) {
        return state
      }

      const lookup = findItem(state.menu, targetId)
      if (!lookup || lookup.parentId || !isSubmenuParent(lookup.item) || lookup.item.disabled) {
        return state
      }

      const childId =
        state.openSubmenuId === lookup.item.id && state.activeSubmenuItemId
          ? state.activeSubmenuItemId
          : firstEnabledId(lookup.item.children.rows)

      return {
        activeItemId: childId ?? lookup.item.id,
        openSubmenuId: lookup.item.id,
        activeSubmenuItemId: childId,
      }
    }),
  activateCurrent: () => {
    const activeItemId = get().activeItemId
    if (!activeItemId) {
      return
    }
    get().activateItem(activeItemId)
  },
  activateItem: (itemId) => {
    const menu = get().menu
    if (!menu) {
      return
    }

    const lookup = findItem(menu, itemId)
    if (!lookup || lookup.item.disabled) {
      return
    }

    if (isSubmenuChildLookup(lookup)) {
      clearNativePlaceholderTimer()
      set(CLOSED_MENU_STATE)
      dispatchContextMenuAction(menu.paneId, lookup.item.action)
      return
    }

    if ('kind' in lookup.item && lookup.item.kind === 'submenu') {
      const childId = firstEnabledId(lookup.item.children.rows)
      set({
        activeItemId: childId ?? lookup.item.id,
        openSubmenuId: lookup.item.id,
        activeSubmenuItemId: childId,
      })
      return
    }

    clearNativePlaceholderTimer()
    set(CLOSED_MENU_STATE)
    dispatchContextMenuAction(menu.paneId, lookup.item.action)
  },
  closeSubmenu: () =>
    set((state) => {
      if (!state.menu || !state.openSubmenuId) {
        return state
      }

      return {
        activeItemId: state.openSubmenuId,
        openSubmenuId: null,
        activeSubmenuItemId: null,
      }
    }),
}))
