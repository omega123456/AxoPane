import { useEffect, useLayoutEffect, useRef } from 'react'
import { ContextMenuSection } from '@/components/menus/ContextMenuSection'
import { ContextMenuTopStrip } from '@/components/menus/ContextMenuTopStrip'
import { log } from '@/lib/app-log-commands'
import { dedupeNativeMenuItems } from '@/lib/context-menu/menu-dedupe'
import { requestNativeMenu } from '@/lib/context-menu/native-menu-commands'
import { useContextMenuStore } from '@/stores/context-menu-store'
import type { ContextMenuDocument } from '@/lib/types/context-menu'
import {
  applyMenuGeometry,
  documentZoom,
  placeMenuAtPoint,
  toZoomedGeometry,
} from '@/components/menus/menu-geometry'
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { handleKeyByTable, type KeyHandlerTable } from '@/lib/key-table'

type MenuSections = {
  before: ContextMenuDocument['sections']
  native: ContextMenuDocument['sections'][number] | null
  after: ContextMenuDocument['sections']
}

function splitAroundNativeSection(menu: ContextMenuDocument): MenuSections {
  const nativeSectionId = menu.nativeSectionId ?? null
  const nativeSectionIndex = nativeSectionId
    ? menu.sections.findIndex((section) => section.id === nativeSectionId)
    : -1
  if (nativeSectionIndex < 0) {
    return { before: menu.sections, native: null, after: [] }
  }
  return {
    before: menu.sections.slice(0, nativeSectionIndex),
    native: menu.sections[nativeSectionIndex],
    after: menu.sections.slice(nativeSectionIndex + 1),
  }
}

function menuKeyTable(openSubmenuId: string | null): KeyHandlerTable {
  const store = useContextMenuStore.getState()
  const submenuKeys: KeyHandlerTable = openSubmenuId
    ? { ArrowLeft: () => store.closeSubmenu(), Escape: () => store.closeSubmenu() }
    : { Escape: () => store.closeMenu() }
  return {
    ArrowDown: () => store.moveActive(1),
    ArrowUp: () => store.moveActive(-1),
    Home: () => store.moveActiveToEdge('start'),
    End: () => store.moveActiveToEdge('end'),
    Enter: () => store.activateCurrent(),
    ' ': () => store.activateCurrent(),
    ArrowRight: () => store.openSubmenu(),
    ...submenuKeys,
  }
}

function useNativeMenuLoad(menu: ContextMenuDocument | null, nativeRequestId: string | null) {
  const beginNativeLoad = useContextMenuStore((state) => state.beginNativeLoad)
  const resolveNativeLoad = useContextMenuStore((state) => state.resolveNativeLoad)
  const failNativeLoad = useContextMenuStore((state) => state.failNativeLoad)

  useEffect(() => {
    if (!menu?.nativeRequest || nativeRequestId) {
      return
    }

    const next = beginNativeLoad()
    if (!next) {
      return
    }

    void requestNativeMenu({
      requestId: next.requestId,
      ...next.request,
    })
      .then((response) => {
        resolveNativeLoad(response.requestId, dedupeNativeMenuItems(response.items))
      })
      .catch((error) => {
        failNativeLoad(next.requestId)
        log.warn('load_native_menu IPC failed', {
          requestId: next.requestId,
          paneId: menu.paneId,
          title: menu.title,
          error,
        })
      })
  }, [beginNativeLoad, failNativeLoad, menu, nativeRequestId, resolveNativeLoad])
}

type NativeSectionState = {
  loading: boolean
  failed: boolean
  placeholderVisible: boolean
  locked: boolean
}

function useMenuFocusAndPlacement(
  menu: ContextMenuDocument | null,
  nativeState: NativeSectionState,
) {
  const ref = useRef<HTMLDivElement | null>(null)
  const { loading, failed, placeholderVisible, locked } = nativeState

  useEffect(() => {
    if (!menu) {
      return
    }

    ref.current?.focus()
  }, [menu, failed, loading, placeholderVisible, locked])

  // Anchor the menu at the cursor, then clamp it into the viewport so a
  // right-click near the right/bottom edge keeps the menu attached to the
  // pointer instead of overflowing off-screen. Writing the clamped geometry
  // straight to the node (rather than via state) keeps this a pure DOM-sync
  // effect with no extra render.
  useLayoutEffect(() => {
    const node = ref.current
    if (!menu || !node) {
      return
    }

    // Measure from the menu's natural content height every time. Otherwise a
    // previously clamped explicit height can get "stuck" after native rows
    // load, leaving the body scroll-cut even when the viewport has room.
    node.style.height = ''
    node.style.maxHeight = ''

    // Flipping around the cursor keeps a corner pinned to the pointer instead
    // of detaching toward the viewport edge.
    const geometry = placeMenuAtPoint(menu.x, menu.y, node.getBoundingClientRect())
    applyMenuGeometry(node, toZoomedGeometry(geometry, documentZoom()))
  }, [menu, failed, loading, placeholderVisible, locked])

  return ref
}

export function ContextMenu() {
  const menu = useContextMenuStore((state) => state.menu)
  const activeItemId = useContextMenuStore((state) => state.activeItemId)
  const openSubmenuId = useContextMenuStore((state) => state.openSubmenuId)
  const nativeRequestId = useContextMenuStore((state) => state.nativeRequestId)
  const nativeState: NativeSectionState = {
    loading: useContextMenuStore((state) => state.nativeLoading),
    failed: useContextMenuStore((state) => state.nativeFailed),
    placeholderVisible: useContextMenuStore((state) => state.nativePlaceholderVisible),
    locked: useContextMenuStore((state) => state.nativeSectionLocked),
  }
  const closeMenu = useContextMenuStore((state) => state.closeMenu)
  const hoverItem = useContextMenuStore((state) => state.hoverItem)
  const activateItem = useContextMenuStore((state) => state.activateItem)
  useNativeMenuLoad(menu, nativeRequestId)
  const ref = useMenuFocusAndPlacement(menu, nativeState)

  if (!menu) {
    return null
  }

  return (
    <div className="fixed inset-0 z-50" onMouseDown={() => closeMenu()}>
      <div
        ref={ref}
        role="menu"
        aria-label={menu.title}
        tabIndex={-1}
        className="absolute flex w-menu max-h-full flex-col overflow-hidden rounded-overlay border border-light-flyout-stroke bg-light-card py-1 shadow-light-flyout focus-visible:outline-none dark:border-dark-flyout-stroke dark:bg-dark-card dark:shadow-dark-flyout"
        // Styling-constraint exception: runtime geometry only. The menu is
        // positioned at the cursor (continuous px coords) and clamped into the
        // viewport by the layout effect above, which no static utility or
        // @theme token can express. All design-system values
        // (color/spacing/typography/radii) above remain pure Tailwind utilities.
        style={{ left: `${menu.x}px`, top: `${menu.y}px` }}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => handleKeyByTable(event, menuKeyTable(openSubmenuId))}
      >
        <ContextMenuTopStrip
          items={menu.topStrip}
          activeItemId={activeItemId}
          onHoverItem={hoverItem}
          onActivateItem={activateItem}
        />
        <ContextMenuBody
          sections={splitAroundNativeSection(menu)}
          nativeState={nativeState}
          activeItemId={activeItemId}
          openSubmenuId={openSubmenuId}
          onHoverItem={hoverItem}
          onActivateItem={activateItem}
        />
      </div>
    </div>
  )
}

type ContextMenuBodyProps = {
  sections: MenuSections
  nativeState: NativeSectionState
  activeItemId: string | null
  openSubmenuId: string | null
  onHoverItem: (itemId: string) => void
  onActivateItem: (itemId: string) => void
}

function ContextMenuBody({ sections, nativeState, ...rowProps }: ContextMenuBodyProps) {
  const { before, native, after } = sections
  return (
    <div
      data-testid="context-menu-scroll-body"
      className={`min-h-0 flex-1 overflow-y-auto overscroll-contain ${SCROLLBAR_THIN}`}
    >
      {before.map((section, index) => (
        <ContextMenuSection
          key={section.id}
          section={section}
          showDivider={index > 0}
          {...rowProps}
        />
      ))}
      {native ? (
        <ContextMenuSection
          section={native}
          showDivider={before.length > 0}
          nativeState={nativeState}
          {...rowProps}
        />
      ) : null}
      {after.map((section, index) => (
        <ContextMenuSection
          key={section.id}
          section={section}
          showDivider={index > 0 || before.length > 0 || native !== null}
          {...rowProps}
        />
      ))}
    </div>
  )
}
