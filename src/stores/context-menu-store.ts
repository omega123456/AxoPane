import { create } from 'zustand'
import { dispatchContextMenuAction } from '@/lib/context-menu/context-menu-actions'
import type { ContextMenuDocument, ContextMenuNativeRequest } from '@/lib/types/context-menu'
import type { NativeMenuItem } from '@/lib/types/ipc'
import {
  activationFor,
  closedSubmenuFocus,
  CLOSED_FOCUS,
  edgeFocus,
  hoveredFocus,
  initialFocus,
  movedFocus,
  openedSubmenuFocus,
  type FocusDirection,
  type FocusEdge,
  type MenuActivation,
  type MenuFocus,
} from '@/stores/context-menu/menu-focus'
import {
  failedNativeLoad,
  IDLE_NATIVE_LOAD,
  isAwaitingNativePlaceholder,
  isCurrentNativeRequest,
  NATIVE_PLACEHOLDER_SHOWN,
  openedMenu,
  resolvedNativeLoad,
  startedNativeLoad,
  type NativeLoad,
} from '@/stores/context-menu/native-load'

const NATIVE_PLACEHOLDER_DELAY_MS = 1000
const NATIVE_REQUEST_ID_PREFIX = 'native-request-'

type Store = MenuFocus &
  NativeLoad & {
    menu: ContextMenuDocument | null
    openMenu: (menu: ContextMenuDocument) => void
    closeMenu: () => void
    beginNativeLoad: () => { requestId: string; request: ContextMenuNativeRequest } | null
    resolveNativeLoad: (requestId: string, items: NativeMenuItem[]) => void
    failNativeLoad: (requestId: string) => void
    hoverItem: (itemId: string) => void
    moveActive: (direction: FocusDirection) => void
    moveActiveToEdge: (edge: FocusEdge) => void
    openSubmenu: (itemId?: string) => void
    activateCurrent: () => void
    activateItem: (itemId: string) => void
    closeSubmenu: () => void
  }

type OpenStore = Store & { menu: ContextMenuDocument }

let nativeRequestSequence = 0
let nativePlaceholderTimer: ReturnType<typeof setTimeout> | null = null

const CLOSED_MENU_STATE = {
  menu: null,
  ...CLOSED_FOCUS,
  ...IDLE_NATIVE_LOAD,
} satisfies Partial<Store>

function clearNativePlaceholderTimer() {
  if (nativePlaceholderTimer === null) {
    return
  }

  clearTimeout(nativePlaceholderTimer)
  nativePlaceholderTimer = null
}

function nextNativeRequestId() {
  nativeRequestSequence += 1
  return `${NATIVE_REQUEST_ID_PREFIX}${nativeRequestSequence}`
}

function showNativePlaceholderIfStillLoading(requestId: string) {
  if (isAwaitingNativePlaceholder(useContextMenuStore.getState(), requestId)) {
    useContextMenuStore.setState(NATIVE_PLACEHOLDER_SHOWN)
  }
}

function canBeginNativeLoad(
  state: Store,
): state is Store & { nativeRequest: ContextMenuNativeRequest } {
  return Boolean(state.menu && state.nativeRequest && !state.nativeRequestId)
}

type StoreSet = (partial: Partial<Store>) => void
type StoreGet = () => Store

function closeAndDispatch(
  set: StoreSet,
  activation: Extract<MenuActivation, { kind: 'dispatch' }>,
) {
  clearNativePlaceholderTimer()
  set(CLOSED_MENU_STATE)
  dispatchContextMenuAction(activation.paneId, activation.action)
}

type StoreAccess = {
  set: StoreSet
  get: StoreGet
}

function finishNativeLoad(
  { set, get }: StoreAccess,
  requestId: string,
  finish: (state: OpenStore) => Partial<Store>,
) {
  const state = get()
  if (!isCurrentNativeRequest(state, requestId)) {
    return
  }
  clearNativePlaceholderTimer()
  set(finish(state))
}

function beginNativeLoad(set: StoreSet, get: StoreGet) {
  const state = get()
  if (!canBeginNativeLoad(state)) {
    return null
  }

  const requestId = nextNativeRequestId()
  clearNativePlaceholderTimer()
  set(startedNativeLoad(requestId))
  nativePlaceholderTimer = setTimeout(
    () => showNativePlaceholderIfStillLoading(requestId),
    NATIVE_PLACEHOLDER_DELAY_MS,
  )
  return { requestId, request: state.nativeRequest }
}

function activateItem(set: StoreSet, get: StoreGet, itemId: string) {
  const menu = get().menu
  const activation = menu ? activationFor(menu, itemId) : null
  if (!activation) {
    return
  }
  if (activation.kind === 'focus') {
    set(activation.focus)
    return
  }
  closeAndDispatch(set, activation)
}

export const useContextMenuStore = create<Store>((set, get) => ({
  ...CLOSED_MENU_STATE,
  openMenu: (menu) => {
    clearNativePlaceholderTimer()
    const opened = openedMenu(menu)
    set({ ...CLOSED_MENU_STATE, ...opened, ...initialFocus(opened.menu) })
  },
  closeMenu: () => {
    clearNativePlaceholderTimer()
    set(CLOSED_MENU_STATE)
  },
  beginNativeLoad: () => beginNativeLoad(set, get),
  resolveNativeLoad: (requestId, items) =>
    finishNativeLoad({ set, get }, requestId, (state) => resolvedNativeLoad(state, items)),
  failNativeLoad: (requestId) => finishNativeLoad({ set, get }, requestId, failedNativeLoad),
  hoverItem: (itemId) => set((state) => hoveredFocus(state, itemId) ?? state),
  moveActive: (direction) => set((state) => movedFocus(state, direction) ?? state),
  moveActiveToEdge: (edge) => set((state) => edgeFocus(state, edge) ?? state),
  openSubmenu: (itemId) => set((state) => openedSubmenuFocus(state, itemId) ?? state),
  activateCurrent: () => {
    const activeItemId = get().activeItemId
    if (!activeItemId) {
      return
    }
    get().activateItem(activeItemId)
  },
  activateItem: (itemId) => activateItem(set, get, itemId),
  closeSubmenu: () => set((state) => closedSubmenuFocus(state) ?? state),
}))
