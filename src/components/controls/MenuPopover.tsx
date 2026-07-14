import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type Ref,
} from 'react'
import { CheckIcon } from '@/components/icons'

export type MenuPopoverItem = {
  id: string
  label: string
  icon?: ReactNode
  disabled?: boolean
  onSelect: () => void
}

export type MenuPopoverRadioItem = MenuPopoverItem & { checked: boolean }

type MenuPopoverSharedProps = {
  ariaLabel: string
  trigger: (props: {
    ref: Ref<HTMLButtonElement>
    expanded: boolean
    controls: string
    toggle: () => void
    onTriggerKeyDown: (event: ReactKeyboardEvent<HTMLButtonElement>) => void
  }) => ReactNode
}

type MenuPopoverProps = MenuPopoverSharedProps &
  ({ items: MenuPopoverRadioItem[]; radio: true } | { items: MenuPopoverItem[]; radio?: false })

/**
 * A compact, trigger-anchored menu for toolbar controls. It intentionally owns
 * its keyboard and focus lifecycle so callers only describe actions.
 */
export function MenuPopover({ ariaLabel, trigger, items, radio = false }: MenuPopoverProps) {
  const [open, setOpen] = useState(false)
  const menuId = useId()
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([])

  const focusItem = (index: number) => itemRefs.current[index]?.focus()
  const enabledIndexes = () => items.flatMap((item, index) => (item.disabled ? [] : [index]))

  function closeAndRestoreFocus() {
    setOpen(false)
    requestAnimationFrame(() => triggerRef.current?.focus())
  }

  function openMenu() {
    setOpen(true)
    requestAnimationFrame(() => {
      const checkedIndex = radio
        ? (items as MenuPopoverRadioItem[]).findIndex((item) => item.checked)
        : -1
      focusItem(checkedIndex >= 0 ? checkedIndex : (enabledIndexes()[0] ?? 0))
    })
  }

  function toggleMenu() {
    if (open) closeAndRestoreFocus()
    else openMenu()
  }

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node
      if (!menuRef.current?.contains(target) && !triggerRef.current?.contains(target))
        closeAndRestoreFocus()
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [open])

  function onMenuKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    const currentIndex = itemRefs.current.findIndex((item) => item === document.activeElement)
    const indexes = enabledIndexes()
    const currentPosition = indexes.indexOf(currentIndex)
    if (event.key === 'Escape') {
      event.preventDefault()
      closeAndRestoreFocus()
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const next = event.key === 'ArrowDown' ? currentPosition + 1 : currentPosition - 1
      focusItem(indexes[(next + indexes.length) % indexes.length] ?? 0)
    } else if (event.key === 'Home') {
      event.preventDefault()
      focusItem(indexes[0] ?? 0)
    } else if (event.key === 'End') {
      event.preventDefault()
      focusItem(indexes.at(-1) ?? 0)
    } else if (event.key === 'Tab') {
      closeAndRestoreFocus()
    }
  }

  function onTriggerKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>) {
    if (!open && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault()
      openMenu()
    }
  }

  return (
    <div className="relative shrink-0">
      {trigger({
        ref: triggerRef,
        expanded: open,
        controls: menuId,
        toggle: toggleMenu,
        onTriggerKeyDown,
      })}
      {open ? (
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-label={ariaLabel}
          onKeyDown={onMenuKeyDown}
          className="absolute right-0 top-full z-30 mt-1 min-w-menu rounded-tab border border-light-border-strong bg-light-panel p-1 shadow-menu dark:border-dark-border-strong dark:bg-dark-panel"
        >
          {items.map((item, index) => {
            const radioItem = item as MenuPopoverRadioItem
            return (
              <button
                key={item.id}
                ref={(element) => {
                  itemRefs.current[index] = element
                }}
                type="button"
                role={radio ? 'menuitemradio' : 'menuitem'}
                aria-checked={radio ? radioItem.checked : undefined}
                disabled={item.disabled}
                onClick={() => {
                  item.onSelect()
                  closeAndRestoreFocus()
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-tab px-2 py-1.5 text-left text-row text-light-text hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border disabled:cursor-not-allowed disabled:opacity-50 dark:text-dark-text dark:hover:bg-dark-hover"
              >
                <span
                  className="flex size-4 shrink-0 items-center justify-center"
                  aria-hidden="true"
                >
                  {radio && radioItem.checked ? (
                    <CheckIcon className="size-3.5 text-accent-blue-light dark:text-accent-blue" />
                  ) : (
                    item.icon
                  )}
                </span>
                {item.label}
              </button>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}
