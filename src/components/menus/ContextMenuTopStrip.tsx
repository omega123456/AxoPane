import { ContextMenuIconGlyph } from '@/components/menus/ContextMenuRow'
import type { ContextMenuStripItem } from '@/lib/types/context-menu'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'

export function ContextMenuTopStrip({
  items,
  activeItemId,
  onHoverItem,
  onActivateItem,
}: {
  items: ContextMenuStripItem[]
  activeItemId: string | null
  onHoverItem: (itemId: string) => void
  onActivateItem: (itemId: string) => void
}) {
  if (items.length === 0) {
    return null
  }

  return (
    <div
      data-context-menu-top-strip="true"
      role="group"
      aria-label="Quick actions"
      className="mb-1 flex items-center justify-between border-b border-light-divider px-2 pb-2 pt-0.5 dark:border-dark-divider"
    >
      {items.map((item) => {
        const active = item.id === activeItemId
        const textClass = item.disabled
          ? 'text-light-text-disabled dark:text-dark-text-disabled'
          : item.danger
            ? 'text-light-danger dark:text-dark-danger'
            : 'text-light-text dark:text-dark-text'

        return (
          <button
            key={item.id}
            type="button"
            role="menuitem"
            aria-label={item.label}
            title={item.label}
            disabled={item.disabled}
            className={`inline-flex h-9 w-10 flex-none items-center justify-center rounded-control ${FOCUS_RING_INSET} ${
              active
                ? 'bg-light-hover dark:bg-dark-hover'
                : 'hover:bg-light-hover dark:hover:bg-dark-hover'
            } ${textClass}`}
            onMouseEnter={() => {
              if (!item.disabled) {
                onHoverItem(item.id)
              }
            }}
            onPointerDown={(event) => {
              if (item.disabled || event.button !== 0) {
                return
              }

              event.preventDefault()
              event.stopPropagation()
              onActivateItem(item.id)
            }}
            onClick={(event) => {
              if (!item.disabled && event.detail === 0) {
                onActivateItem(item.id)
              }
            }}
          >
            {item.icon ? <ContextMenuIconGlyph icon={item.icon} className="size-4" /> : null}
          </button>
        )
      })}
    </div>
  )
}
