import type { ContextMenuDocument, ContextMenuNativeRequest } from '@/lib/types/context-menu'
import type { NativeMenuItem } from '@/lib/types/ipc'
import {
  EMPTY_NATIVE_ROWS,
  injectNativeSection,
  mapNativeItem,
  replaceNativeSectionRows,
  sanitizeMenu,
} from '@/stores/context-menu/menu-document'

export type NativeLoad = {
  nativeRequest: ContextMenuNativeRequest | null
  nativeRequestId: string | null
  nativeLoading: boolean
  nativeFailed: boolean
  nativePlaceholderVisible: boolean
  nativeSectionLocked: boolean
}

type NativeLoadState = NativeLoad & { menu: ContextMenuDocument | null }

type NativeLoadResult = NativeLoad & { menu: ContextMenuDocument }

export const IDLE_NATIVE_LOAD: NativeLoad = {
  nativeRequest: null,
  nativeRequestId: null,
  nativeLoading: false,
  nativeFailed: false,
  nativePlaceholderVisible: false,
  nativeSectionLocked: false,
}

export const NATIVE_PLACEHOLDER_SHOWN = {
  nativePlaceholderVisible: true,
  nativeSectionLocked: true,
} satisfies Partial<NativeLoad>

export function openedMenu(menu: ContextMenuDocument) {
  const sanitized = sanitizeMenu(injectNativeSection(menu))
  return { menu: sanitized, nativeRequest: sanitized.nativeRequest ?? null }
}

export function startedNativeLoad(requestId: string) {
  return {
    nativeRequestId: requestId,
    nativeLoading: true,
    nativeFailed: false,
    nativePlaceholderVisible: false,
    nativeSectionLocked: false,
  } satisfies Partial<NativeLoad>
}

export function isCurrentNativeRequest(
  state: NativeLoadState,
  requestId: string,
): state is NativeLoadState & { menu: ContextMenuDocument } {
  return state.menu !== null && state.nativeRequestId === requestId
}

export function isAwaitingNativePlaceholder(state: NativeLoad, requestId: string) {
  return state.nativeRequestId === requestId && state.nativeLoading
}

function settledNativeLoad(
  menu: ContextMenuDocument,
  outcome: Pick<NativeLoad, 'nativeFailed' | 'nativeSectionLocked'>,
): NativeLoadResult {
  return { ...IDLE_NATIVE_LOAD, ...outcome, menu: sanitizeMenu(menu) }
}

export function resolvedNativeLoad(
  state: NativeLoadState & { menu: ContextMenuDocument },
  items: NativeMenuItem[],
): NativeLoadResult {
  const rows = items.map(mapNativeItem)
  return settledNativeLoad(replaceNativeSectionRows(state.menu, rows), {
    nativeFailed: false,
    nativeSectionLocked: state.nativeSectionLocked || rows.length > 0,
  })
}

export function failedNativeLoad(
  state: NativeLoadState & { menu: ContextMenuDocument },
): NativeLoadResult {
  return settledNativeLoad(replaceNativeSectionRows(state.menu, EMPTY_NATIVE_ROWS), {
    nativeFailed: true,
    nativeSectionLocked: state.nativeSectionLocked,
  })
}
