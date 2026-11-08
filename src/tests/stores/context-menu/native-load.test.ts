import { describe, expect, it } from 'vitest'
import { noopContextAction } from '@/lib/context-menu/context-menu-actions'
import type { ContextMenuDocument } from '@/lib/types/context-menu'
import type { NativeMenuItem } from '@/lib/types/ipc'
import {
  injectNativeSection,
  mapNativeItem,
  replaceNativeSectionRows,
  sanitizeMenu,
} from '@/stores/context-menu/menu-document'
import {
  failedNativeLoad,
  IDLE_NATIVE_LOAD,
  isAwaitingNativePlaceholder,
  isCurrentNativeRequest,
  openedMenu,
  resolvedNativeLoad,
  startedNativeLoad,
} from '@/stores/context-menu/native-load'

const noop = noopContextAction('noop')
const NATIVE_SECTION = 'native'
const NATIVE_REQUEST = { targetKind: 'file', paths: ['/a'] } as never

function menuDocument(overrides: Partial<ContextMenuDocument> = {}): ContextMenuDocument {
  return {
    paneId: 'left',
    x: 0,
    y: 0,
    title: 'Menu',
    topStrip: [
      { id: 'cut', label: 'Cut', owner: 'app', action: noop },
      { id: 'hidden', label: 'Hidden', owner: 'app', action: noop, hidden: true },
    ],
    sections: [
      {
        id: 'first',
        rows: [{ id: 'open', label: 'Open', owner: 'app', kind: 'action', action: noop }],
      },
      {
        id: 'last',
        rows: [{ id: 'props', label: 'Props', owner: 'app', kind: 'action', action: noop }],
      },
    ],
    nativeRequest: NATIVE_REQUEST,
    nativeSectionId: NATIVE_SECTION,
    ...overrides,
  }
}

function nativeItem(overrides: Partial<NativeMenuItem> = {}): NativeMenuItem {
  return {
    id: 'share',
    label: 'Share',
    enabled: true,
    danger: false,
    canonicalActionKind: null,
    normalizedVerb: null,
    invokeToken: 'token',
    icon: null,
    children: [],
    ...overrides,
  }
}

describe('menu-document', () => {
  it('reserves the native section above the last section only once', () => {
    const injected = injectNativeSection(menuDocument())
    expect(injected.sections.map((section) => section.id)).toEqual([
      'first',
      NATIVE_SECTION,
      'last',
    ])
    expect(injectNativeSection(injected)).toBe(injected)
    const withoutRequest = menuDocument({ nativeRequest: null })
    expect(injectNativeSection(withoutRequest)).toBe(withoutRequest)
  })

  it('drops hidden rows, empty submenus and empty sections but keeps the native slot', () => {
    const menu = menuDocument({
      sections: [
        { id: NATIVE_SECTION, rows: [] },
        {
          id: 'empty',
          rows: [
            { id: 'gone', label: 'Gone', owner: 'app', kind: 'action', action: noop, hidden: true },
          ],
        },
        {
          id: 'sub',
          rows: [
            {
              id: 'more',
              label: 'More',
              owner: 'app',
              kind: 'submenu',
              children: {
                id: 'panel',
                rows: [{ id: 'child', label: 'Child', owner: 'app', action: noop, hidden: true }],
              },
            },
          ],
        },
      ],
    })
    const sanitized = sanitizeMenu(menu)
    expect(sanitized.topStrip.map((item) => item.id)).toEqual(['cut'])
    expect(sanitized.sections.map((section) => section.id)).toEqual([NATIVE_SECTION])
  })

  it('replaces only the native section rows', () => {
    const menu = injectNativeSection(menuDocument())
    const rows = [mapNativeItem(nativeItem())]
    const replaced = replaceNativeSectionRows(menu, rows)
    expect(replaced.sections.find((section) => section.id === NATIVE_SECTION)?.rows).toBe(rows)
    const withoutNative = menuDocument({ nativeSectionId: null })
    expect(replaceNativeSectionRows(withoutNative, rows)).toBe(withoutNative)
  })

  it('maps native items into actions, submenus and unavailable rows', () => {
    expect(mapNativeItem(nativeItem())).toMatchObject({
      kind: 'action',
      disabled: false,
      action: { kind: 'invoke-native', token: 'token' },
    })
    expect(mapNativeItem(nativeItem({ invokeToken: null }))).toMatchObject({
      kind: 'action',
      disabled: true,
      action: { kind: 'noop', debugId: 'native-unavailable-share' },
    })
    const submenu = mapNativeItem(
      nativeItem({
        invokeToken: null,
        icon: { kind: 'dataUrl', dataUrl: 'data:x', alt: null },
        children: [nativeItem({ id: 'child' }), nativeItem({ id: 'dead', invokeToken: null })],
      }),
    )
    expect(submenu).toMatchObject({
      kind: 'submenu',
      disabled: false,
      icon: { kind: 'native', dataUrl: 'data:x', alt: undefined },
      children: { id: 'share-submenu', rows: [{ id: 'child' }] },
    })
  })
})

describe('native-load', () => {
  it('opens a sanitized menu carrying its native request', () => {
    const opened = openedMenu(menuDocument())
    expect(opened.nativeRequest).toBe(NATIVE_REQUEST)
    expect(opened.menu.sections.map((section) => section.id)).toContain(NATIVE_SECTION)
    expect(openedMenu(menuDocument({ nativeRequest: undefined })).nativeRequest).toBeNull()
  })

  it('tracks which request is current and awaiting a placeholder', () => {
    const loading = { ...IDLE_NATIVE_LOAD, ...startedNativeLoad('r1'), menu: menuDocument() }
    expect(isCurrentNativeRequest(loading, 'r1')).toBe(true)
    expect(isCurrentNativeRequest(loading, 'r2')).toBe(false)
    expect(isCurrentNativeRequest({ ...loading, menu: null }, 'r1')).toBe(false)
    expect(isAwaitingNativePlaceholder(loading, 'r1')).toBe(true)
    expect(isAwaitingNativePlaceholder({ ...loading, nativeLoading: false }, 'r1')).toBe(false)
  })

  it('locks the section once native rows resolve and keeps the lock on failure', () => {
    const loading = {
      ...IDLE_NATIVE_LOAD,
      ...startedNativeLoad('r1'),
      menu: injectNativeSection(menuDocument()),
    }
    const resolved = resolvedNativeLoad(loading, [nativeItem()])
    expect(resolved).toMatchObject({
      nativeRequestId: null,
      nativeLoading: false,
      nativeSectionLocked: true,
    })
    expect(resolvedNativeLoad(loading, []).nativeSectionLocked).toBe(false)
    const failed = failedNativeLoad({ ...loading, nativeSectionLocked: true })
    expect(failed).toMatchObject({
      nativeFailed: true,
      nativeSectionLocked: true,
      nativeLoading: false,
    })
  })
})
