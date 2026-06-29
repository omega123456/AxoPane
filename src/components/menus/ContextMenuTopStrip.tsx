import { ContextMenuIconGlyph } from '@/components/menus/ContextMenuRow'
import type { ContextMenuStripItem } from '@/lib/types/context-menu'

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
      className="-mx-1.5 -mt-1.5 mb-1 flex h-12 items-center justify-between border-b border-light-border bg-light-surface px-2 dark:border-dark-border dark:bg-dark-surface"
    >
      {items.map((item) => {
        const active = item.id === activeItemId
        const textClass = item.disabled
          ? 'text-light-text-faint dark:text-dark-text-faint'
          : item.danger
            ? 'text-accent-red'
            : 'text-light-text dark:text-dark-text'

        return (
          <button
            key={item.id}
            type="button"
            role="menuitem"
            aria-label={item.label}
            title={item.label}
            disabled={item.disabled}
            className={`inline-flex size-10 flex-none items-center justify-center rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border ${
              active ? 'bg-accent-blue-soft' : 'hover:bg-light-hover dark:hover:bg-dark-hover'
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
            {item.icon ? <ContextMenuIconGlyph icon={item.icon} className="size-5" /> : null}
          </button>
        )
      })}
    </div>
  )
}
