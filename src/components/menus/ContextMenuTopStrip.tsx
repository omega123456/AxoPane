import { ContextMenuIconGlyph } from '@/components/menus/ContextMenuRow'
import type { ContextMenuStripItem } from '@/lib/types/context-menu'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { menuItemStateClass } from '@/lib/class-names'
import { PRIMARY_MOUSE_BUTTON } from '@/lib/mouse-buttons'

const KEYBOARD_CLICK_DETAIL = 0

type StripHandlers = {
  onHoverItem: (itemId: string) => void
  onActivateItem: (itemId: string) => void
}

function stripTextClass(item: ContextMenuStripItem) {
  if (item.disabled) {
    return 'text-light-text-disabled dark:text-dark-text-disabled'
  }
  if (item.danger) {
    return 'text-light-danger dark:text-dark-danger'
  }
  return 'text-light-text dark:text-dark-text'
}

export function ContextMenuTopStrip({
  items,
  activeItemId,
  ...handlers
}: StripHandlers & {
  items: ContextMenuStripItem[]
  activeItemId: string | null
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
      {items.map((item) => (
        <StripButton key={item.id} item={item} active={item.id === activeItemId} {...handlers} />
      ))}
    </div>
  )
}

function StripButton({
  item,
  active,
  onHoverItem,
  onActivateItem,
}: StripHandlers & { item: ContextMenuStripItem; active: boolean }) {
  return (
    <button
      type="button"
      role="menuitem"
      aria-label={item.label}
      title={item.label}
      disabled={item.disabled}
      className={`inline-flex h-9 w-10 flex-none items-center justify-center rounded-control ${FOCUS_RING_INSET} ${menuItemStateClass(active)} ${stripTextClass(item)}`}
      onMouseEnter={() => {
        if (!item.disabled) {
          onHoverItem(item.id)
        }
      }}
      onPointerDown={(event) => {
        if (item.disabled) {
          return
        }
        if (event.button !== PRIMARY_MOUSE_BUTTON) {
          return
        }

        event.preventDefault()
        event.stopPropagation()
        onActivateItem(item.id)
      }}
      onClick={(event) => {
        if (item.disabled) {
          return
        }
        if (event.detail === KEYBOARD_CLICK_DETAIL) {
          onActivateItem(item.id)
        }
      }}
    >
      {item.icon ? <ContextMenuIconGlyph icon={item.icon} className="size-4" /> : null}
    </button>
  )
}
