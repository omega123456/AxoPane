import { useEffect, useLayoutEffect, useRef } from 'react'
import { ContextMenuSection } from '@/components/menus/ContextMenuSection'
import { ContextMenuTopStrip } from '@/components/menus/ContextMenuTopStrip'
import { log } from '@/lib/app-log-commands'
import { dedupeNativeMenuItems } from '@/lib/context-menu/menu-dedupe'
import { requestNativeMenu } from '@/lib/context-menu/native-menu-commands'
import { useContextMenuStore } from '@/stores/context-menu-store'

export function ContextMenu() {
  const menu = useContextMenuStore((state) => state.menu)
  const activeItemId = useContextMenuStore((state) => state.activeItemId)
  const openSubmenuId = useContextMenuStore((state) => state.openSubmenuId)
  const nativeRequestId = useContextMenuStore((state) => state.nativeRequestId)
  const nativeLoading = useContextMenuStore((state) => state.nativeLoading)
  const nativeFailed = useContextMenuStore((state) => state.nativeFailed)
  const nativePlaceholderVisible = useContextMenuStore((state) => state.nativePlaceholderVisible)
  const nativeSectionLocked = useContextMenuStore((state) => state.nativeSectionLocked)
  const beginNativeLoad = useContextMenuStore((state) => state.beginNativeLoad)
  const resolveNativeLoad = useContextMenuStore((state) => state.resolveNativeLoad)
  const failNativeLoad = useContextMenuStore((state) => state.failNativeLoad)
  const closeMenu = useContextMenuStore((state) => state.closeMenu)
  const closeSubmenu = useContextMenuStore((state) => state.closeSubmenu)
  const openSubmenu = useContextMenuStore((state) => state.openSubmenu)
  const hoverItem = useContextMenuStore((state) => state.hoverItem)
  const moveActive = useContextMenuStore((state) => state.moveActive)
  const moveActiveToEdge = useContextMenuStore((state) => state.moveActiveToEdge)
  const activateCurrent = useContextMenuStore((state) => state.activateCurrent)
  const activateItem = useContextMenuStore((state) => state.activateItem)
  const ref = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!menu) {
      return
    }

    ref.current?.focus()
  }, [menu, nativeFailed, nativeLoading, nativePlaceholderVisible, nativeSectionLocked])

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

    // The app zooms the UI via CSS `zoom` on <html> (see layout-store). Pointer
    // coordinates (`menu.x/y`), `getBoundingClientRect()` and `innerWidth/
    // Height` are all in real viewport pixels, but `left`/`top` written on a
    // node inside the zoomed root are interpreted in *zoomed* units (the browser
    // multiplies them by the zoom factor). So we do all the geometry math in
    // real pixels, then divide the final offsets by the zoom factor.
    const zoom = Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
    const { width, height } = node.getBoundingClientRect()
    const margin = 8
    const maxLeft = window.innerWidth - width - margin
    const maxTop = window.innerHeight - height - margin

    // Flip the menu around the cursor when it would overflow, so a corner stays
    // pinned to the pointer instead of detaching toward the viewport edge.
    let left = menu.x
    if (left > maxLeft) {
      left = menu.x - width
    }
    left = Math.max(margin, Math.min(left, maxLeft))

    let top = menu.y
    if (top > maxTop) {
      top = menu.y - height
    }
    top = Math.max(margin, Math.min(top, maxTop))

    node.style.left = `${left / zoom}px`
    node.style.top = `${top / zoom}px`
  }, [menu])

  if (!menu) {
    return null
  }

  const nativeSectionId = menu.nativeSectionId ?? null
  const nativeSectionIndex = nativeSectionId
    ? menu.sections.findIndex((section) => section.id === nativeSectionId)
    : -1
  const beforeNativeSections =
    nativeSectionIndex >= 0 ? menu.sections.slice(0, nativeSectionIndex) : menu.sections
  const nativeSection = nativeSectionIndex >= 0 ? menu.sections[nativeSectionIndex] : null
  const afterNativeSections = nativeSectionIndex >= 0 ? menu.sections.slice(nativeSectionIndex + 1) : []
  const topStripVisible = menu.topStrip.length > 0

  return (
    <div className="fixed inset-0 z-50" onMouseDown={() => closeMenu()}>
      <div
        ref={ref}
        role="menu"
        aria-label={menu.title}
        tabIndex={-1}
        className="absolute w-72 rounded-menu border border-light-border-strong bg-light-surface p-1.5 shadow-menu focus-visible:outline-none dark:border-dark-border-strong dark:bg-dark-surface"
        // Styling-constraint exception: runtime geometry only. The menu is
        // positioned at the cursor (continuous px coords) and clamped into the
        // viewport by the layout effect above, which no static utility or
        // @theme token can express. All design-system values
        // (color/spacing/typography/radii) above remain pure Tailwind utilities.
        style={{ left: `${menu.x}px`, top: `${menu.y}px` }}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown') {
            event.preventDefault()
            moveActive(1)
          } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            moveActive(-1)
          } else if (event.key === 'Home') {
            event.preventDefault()
            moveActiveToEdge('start')
          } else if (event.key === 'End') {
            event.preventDefault()
            moveActiveToEdge('end')
          } else if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            activateCurrent()
          } else if (event.key === 'ArrowRight') {
            event.preventDefault()
            openSubmenu()
          } else if (event.key === 'ArrowLeft') {
            if (openSubmenuId) {
              event.preventDefault()
              closeSubmenu()
            }
          } else if (event.key === 'Escape') {
            event.preventDefault()
            if (openSubmenuId) {
              closeSubmenu()
            } else {
              closeMenu()
            }
          }
        }}
      >
        <ContextMenuTopStrip
          items={menu.topStrip}
          activeItemId={activeItemId}
          onHoverItem={hoverItem}
          onActivateItem={activateItem}
        />
        {beforeNativeSections.map((section, index) => (
          <ContextMenuSection
            key={section.id}
            section={section}
            showDivider={index > 0 || topStripVisible}
            activeItemId={activeItemId}
            openSubmenuId={openSubmenuId}
            onHoverItem={hoverItem}
            onActivateItem={activateItem}
          />
        ))}
        {nativeSection ? (
          <ContextMenuSection
            section={nativeSection}
            showDivider={beforeNativeSections.length > 0 || topStripVisible}
            activeItemId={activeItemId}
            openSubmenuId={openSubmenuId}
            onHoverItem={hoverItem}
            onActivateItem={activateItem}
            nativeState={{
              loading: nativeLoading,
              failed: nativeFailed,
              placeholderVisible: nativePlaceholderVisible,
              locked: nativeSectionLocked,
            }}
          />
        ) : null}
        {afterNativeSections.map((section, index) => (
          <ContextMenuSection
            key={section.id}
            section={section}
            showDivider={
              index > 0 ||
              topStripVisible ||
              beforeNativeSections.length > 0 ||
              nativeSection !== null
            }
            activeItemId={activeItemId}
            openSubmenuId={openSubmenuId}
            onHoverItem={hoverItem}
            onActivateItem={activateItem}
          />
        ))}
      </div>
    </div>
  )
}
