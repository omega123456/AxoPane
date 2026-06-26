import { useEffect, useLayoutEffect, useRef } from 'react'
import { ChevronRightIcon } from '@/components/icons'
import { useContextMenuStore } from '@/stores/context-menu-store'

export function ContextMenu() {
  const menu = useContextMenuStore((state) => state.menu)
  const activeIndex = useContextMenuStore((state) => state.activeIndex)
  const closeMenu = useContextMenuStore((state) => state.closeMenu)
  const moveActive = useContextMenuStore((state) => state.moveActive)
  const activateCurrent = useContextMenuStore((state) => state.activateCurrent)
  const ref = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!menu) {
      return
    }

    ref.current?.focus()
  }, [menu])

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

  return (
    <div className="fixed inset-0 z-50" onMouseDown={() => closeMenu()}>
      <div
        ref={ref}
        role="menu"
        aria-label={menu.title}
        tabIndex={-1}
        className="absolute w-59 rounded-menu border border-light-border-strong bg-light-window p-1 shadow-menu focus-visible:outline-none dark:border-dark-border-strong dark:bg-dark-window"
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
          } else if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            activateCurrent()
          } else if (event.key === 'Escape') {
            event.preventDefault()
            closeMenu()
          }
        }}
      >
        {menu.chip ? (
          <div className="mb-1 flex items-center gap-2 border-b border-light-border px-2.5 pb-2 pt-1.5 dark:border-dark-border">
            <span className="inline-flex h-4 min-w-8 flex-none items-center justify-center rounded-sm border border-accent-blue-border bg-accent-blue-soft px-1 font-mono text-2xs font-bold text-accent-blue-light dark:text-accent-blue">
              {menu.chip}
            </span>
            <span className="truncate text-usm font-semibold text-light-text dark:text-dark-text">{menu.title}</span>
          </div>
        ) : null}
        {menu.items.map((item, index) => (
          <div key={item.id}>
            {item.separatorBefore ? (
              <div className="mx-1.5 my-1 h-px bg-light-border dark:bg-dark-border" />
            ) : null}
            <button
              type="button"
              role="menuitem"
              disabled={item.disabled}
              className={`flex h-7.5 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-row focus-visible:outline-none ${
                item.strong ? 'font-semibold' : ''
              } ${
                item.disabled
                  ? 'text-light-text-faint dark:text-dark-text-faint'
                  : item.danger
                    ? 'text-accent-red'
                    : 'text-light-text dark:text-dark-text'
              } ${index === activeIndex ? 'bg-accent-blue-soft' : 'hover:bg-light-hover dark:hover:bg-dark-hover'}`}
              onMouseEnter={() => {
                if (!item.disabled) {
                  useContextMenuStore.setState({ activeIndex: index })
                }
              }}
              onClick={() => {
                if (item.disabled) {
                  return
                }
                item.onSelect?.()
                closeMenu()
              }}
            >
              <span className="flex-1 truncate">{item.label}</span>
              {item.shortcut ? (
                <span className="flex-none font-mono text-2xs text-light-text-faint dark:text-dark-text-faint">
                  {item.shortcut}
                </span>
              ) : null}
              {item.submenu ? (
                <ChevronRightIcon className="size-3 flex-none text-light-text-faint dark:text-dark-text-faint" />
              ) : null}
            </button>
          </div>
        ))}
      </div>
    </div>
  )
}
