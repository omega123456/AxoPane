import { useEffect, useRef } from 'react'
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
        className="absolute min-w-56 rounded-window border border-light-border-strong bg-light-panel py-1 shadow-window focus-visible:outline-none dark:border-dark-border-strong dark:bg-dark-panel"
        // Styling-constraint exception: runtime geometry only. The menu is
        // positioned at the cursor (continuous px coords), which no static
        // utility or @theme token can express. All design-system values
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
        {menu.items.map((item, index) => (
          <button
            key={item.id}
            type="button"
            role="menuitem"
            disabled={item.disabled}
            className={`flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-row focus-visible:outline-none ${
              item.disabled
                ? 'text-light-text-muted dark:text-dark-text-muted'
                : item.danger
                  ? 'text-red-600 dark:text-red-400'
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
            <span>{item.label}</span>
            <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
              {item.shortcut ?? ''}
            </span>
          </button>
        ))}
      </div>
    </div>
  )
}
