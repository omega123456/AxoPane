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
  const sectionRef = useRef<HTMLDivElement | null>(null)
  const submenuRef = useRef<HTMLDivElement | null>(null)
  const [openSubmenuAnchor, setOpenSubmenuAnchor] = useState<HTMLButtonElement | null>(null)
  const [openSubmenuPosition, setOpenSubmenuPosition] = useState<{
    left: number
    top: number
  } | null>(null)
  const keepNativeSection = hasNativeRows || nativeState?.loading || nativeState?.locked

  const openSubmenuItem = section.rows.find(
    (item): item is ContextMenuSubmenuRowItem =>
      isContextMenuSubmenuRow(item) && item.id === openSubmenuId,
  )

  useLayoutEffect(() => {
    if (!openSubmenuAnchor || !sectionRef.current || !submenuRef.current || !openSubmenuItem) {
      setOpenSubmenuPosition(null)
      return
    }

    const zoom = Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
    const margin = 8
    const gap = 8
    const anchorRect = openSubmenuAnchor.getBoundingClientRect()
    const sectionRect = sectionRef.current.getBoundingClientRect()
    const submenuRect = submenuRef.current.getBoundingClientRect()
    const minLeft = margin - sectionRect.left
    const maxLeft = window.innerWidth - margin - submenuRect.width - sectionRect.left
    let left = anchorRect.right - sectionRect.left + gap
    if (anchorRect.right + gap + submenuRect.width > window.innerWidth - margin) {
      left = anchorRect.left - sectionRect.left - submenuRect.width - gap
    }
    left = Math.max(minLeft, Math.min(left, maxLeft))

    const minTop = margin - sectionRect.top
    const maxTop = window.innerHeight - margin - submenuRect.height - sectionRect.top
    const top = Math.max(minTop, Math.min(anchorRect.top - sectionRect.top, maxTop))

    setOpenSubmenuPosition((current) => {
      const next = {
        left: left / zoom,
        top: top / zoom,
      }
      if (current && current.left === next.left && current.top === next.top) {
        return current
      }
      return next
    })
  }, [openSubmenuAnchor, openSubmenuItem])

  if (isNativeSection && !keepNativeSection) {
    return null
  }

  return (
    <div ref={sectionRef} className="relative">
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
          className="absolute z-10 w-64 rounded-menu border border-light-border-strong bg-light-surface p-1.5 shadow-menu dark:border-dark-border-strong dark:bg-dark-surface"
          // Runtime submenu geometry mirrors the root menu logic: the panel is
          // measured in viewport pixels, clamped to the viewport, and then
          // translated back into the zoomed coordinate space of the menu tree.
          style={
            openSubmenuPosition
              ? {
                  left: `${openSubmenuPosition.left}px`,
                  top: `${openSubmenuPosition.top}px`,
                }
              : { visibility: 'hidden' }
          }
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
      ) : null}
    </div>
  )
}
