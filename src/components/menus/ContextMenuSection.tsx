import { useLayoutEffect, useRef, useState } from 'react'
import { ContextMenuRow } from '@/components/menus/ContextMenuRow'
import {
  isContextMenuSubmenuRow,
  type ContextMenuSubmenuRowItem,
  type ContextMenuSection as ContextMenuSectionModel,
} from '@/lib/types/context-menu'

export function ContextMenuSection({
  section,
  showDivider,
  activeItemId,
  openSubmenuId,
  onHoverItem,
  onActivateItem,
  nativeState,
}: {
  section: ContextMenuSectionModel
  showDivider: boolean
  activeItemId: string | null
  openSubmenuId: string | null
  onHoverItem: (itemId: string) => void
  onActivateItem: (itemId: string) => void
  nativeState?: {
    loading: boolean
    failed: boolean
    placeholderVisible: boolean
    locked: boolean
  }
}) {
  const showNativePlaceholder = Boolean(nativeState?.placeholderVisible)
  const isNativeSection = nativeState !== undefined
  const hasNativeRows = section.rows.length > 0
  const submenuRef = useRef<HTMLDivElement | null>(null)
  const [openSubmenuAnchor, setOpenSubmenuAnchor] = useState<HTMLButtonElement | null>(null)
  const [openSubmenuPosition, setOpenSubmenuPosition] = useState<{
    left: number
    top: number
    height: number
    maxHeight: number
  } | null>(null)
  const keepNativeSection = hasNativeRows || nativeState?.loading || nativeState?.locked

  const openSubmenuItem = section.rows.find(
    (item): item is ContextMenuSubmenuRowItem =>
      isContextMenuSubmenuRow(item) && item.id === openSubmenuId,
  )

  useLayoutEffect(() => {
    if (!openSubmenuAnchor || !submenuRef.current || !openSubmenuItem) {
      setOpenSubmenuPosition(null)
      return
    }

    submenuRef.current.style.height = ''
    submenuRef.current.style.maxHeight = ''

    const zoom = Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
    const margin = 8
    const gap = 8
    const anchorRect = openSubmenuAnchor.getBoundingClientRect()
    const submenuRect = submenuRef.current.getBoundingClientRect()
    const availableHeight = Math.max(window.innerHeight - margin * 2, 0)
    const clampedHeight = Math.min(submenuRect.height, availableHeight)
    const maxLeft = window.innerWidth - margin - submenuRect.width
    let left = anchorRect.right + gap
    if (anchorRect.right + gap + submenuRect.width > window.innerWidth - margin) {
      left = anchorRect.left - submenuRect.width - gap
    }
    left = Math.max(margin, Math.min(left, maxLeft))

    const maxTop = window.innerHeight - margin - clampedHeight
    const top = Math.max(margin, Math.min(anchorRect.top, maxTop))

    setOpenSubmenuPosition((current) => {
      const next = {
        left: left / zoom,
        top: top / zoom,
        height: clampedHeight / zoom,
        maxHeight: availableHeight / zoom,
      }
      if (
        current &&
        current.left === next.left &&
        current.top === next.top &&
        current.height === next.height &&
        current.maxHeight === next.maxHeight
      ) {
        return current
      }
      return next
    })
  }, [openSubmenuAnchor, openSubmenuItem])

  if (isNativeSection && !keepNativeSection) {
    return null
  }

  return (
    <div>
      {showDivider ? (
        <div className="mx-1.5 my-1 h-px bg-light-border dark:bg-dark-border" />
      ) : null}
      <div className="px-1">
        {isNativeSection && !hasNativeRows && nativeState?.loading ? (
          <div
            aria-hidden={!showNativePlaceholder}
            role={showNativePlaceholder ? 'status' : undefined}
            aria-label={showNativePlaceholder ? 'Loading native menu items' : undefined}
            className="flex min-h-24 flex-col justify-center gap-2 overflow-hidden px-2"
          >
            <div
              className={`h-3 rounded-tab bg-light-skeleton dark:bg-dark-skeleton ${
                showNativePlaceholder ? 'animate-pulse' : 'opacity-0'
              }`}
            />
            <div
              className={`h-3 rounded-tab bg-light-skeleton-strong dark:bg-dark-skeleton-strong ${
                showNativePlaceholder ? 'animate-pulse' : 'opacity-0'
              }`}
            />
          </div>
        ) : null}
        {isNativeSection && !hasNativeRows && !nativeState?.loading && nativeState?.locked ? (
          <div aria-hidden="true" className="min-h-24 px-2" />
        ) : null}
        {section.rows.map((item) => {
          const isSubmenu = isContextMenuSubmenuRow(item)
          const submenuOpen = isSubmenu && item.id === openSubmenuId

          return (
            <ContextMenuRow
              key={item.id}
              item={item}
              active={item.id === activeItemId}
              submenuOpen={submenuOpen}
              buttonRef={submenuOpen ? setOpenSubmenuAnchor : undefined}
              onPointerEnter={() => onHoverItem(item.id)}
              onActivate={() => onActivateItem(item.id)}
            />
          )
        })}
      </div>
      {openSubmenuItem ? (
        <div
          ref={submenuRef}
          role="menu"
          aria-label={openSubmenuItem.label}
          className="fixed z-10 flex w-64 flex-col overflow-hidden rounded-menu border border-light-border-strong bg-light-surface p-1.5 shadow-menu dark:border-dark-border-strong dark:bg-dark-surface"
          // Runtime submenu geometry uses viewport coordinates so nested panels
          // are not clipped by the scrolling body of the parent menu.
          style={
            openSubmenuPosition
              ? {
                  left: `${openSubmenuPosition.left}px`,
                  top: `${openSubmenuPosition.top}px`,
                  height: `${openSubmenuPosition.height}px`,
                  maxHeight: `${openSubmenuPosition.maxHeight}px`,
                }
              : { visibility: 'hidden' }
          }
        >
          <div
            data-testid="context-submenu-scroll-body"
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
          >
            {openSubmenuItem.children.rows.map((child) => (
              <ContextMenuRow
                key={child.id}
                item={child}
                active={child.id === activeItemId}
                onPointerEnter={() => onHoverItem(child.id)}
                onActivate={() => onActivateItem(child.id)}
              />
            ))}
          </div>
        </div>
      ) : null}
    </div>
  )
}
