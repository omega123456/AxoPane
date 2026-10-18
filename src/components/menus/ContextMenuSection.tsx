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
      {showDivider ? <div className="my-1 h-px bg-light-divider dark:bg-dark-divider" /> : null}
      {/* The menu's 4px side padding lives here so dividers can span edge to edge. */}
      <div className="px-1">
        {isNativeSection && !hasNativeRows && nativeState?.loading ? (
          <div
            aria-hidden={!showNativePlaceholder}
            role={showNativePlaceholder ? 'status' : undefined}
            aria-label={showNativePlaceholder ? 'Loading native menu items' : undefined}
            className={`flex flex-col ${showNativePlaceholder ? '' : 'opacity-0'}`}
          >
            {/* Same 32px height as the rows that replace them, so nothing jumps. */}
            {['w-3/5', 'w-4/5', 'w-1/2'].map((width) => (
              <div key={width} className="flex h-8 items-center gap-3 px-3">
                <div className="size-4 rounded-control bg-light-skeleton dark:bg-dark-skeleton" />
                <div
                  className={`h-2.5 ${width} rounded-control bg-light-skeleton dark:bg-dark-skeleton`}
                />
              </div>
            ))}
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
          className="fixed z-10 flex w-menu flex-col overflow-hidden rounded-overlay border border-light-flyout-stroke bg-light-card p-1 shadow-light-flyout dark:border-dark-flyout-stroke dark:bg-dark-card dark:shadow-dark-flyout"
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
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary"
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
