import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type Ref,
} from 'react'
import { createPortal } from 'react-dom'
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

  useLayoutEffect(() => {
    const menu = menuRef.current
    const trigger = triggerRef.current
    if (!open || !menu || !trigger) return

    const zoom = Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
    const triggerRect = trigger.getBoundingClientRect()
    const menuRect = menu.getBoundingClientRect()
    const margin = 8
    const left = Math.max(
      margin,
      Math.min(triggerRect.left, window.innerWidth - menuRect.width - margin),
    )
    const top =
      triggerRect.bottom + menuRect.height <= window.innerHeight - margin
        ? triggerRect.bottom
        : triggerRect.top - menuRect.height

    menu.style.left = `${left / zoom}px`
    menu.style.top = `${Math.max(margin, top) / zoom}px`
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
      {open
        ? createPortal(
            <div
              ref={menuRef}
              id={menuId}
              role="menu"
              aria-label={ariaLabel}
              onKeyDown={onMenuKeyDown}
              className="fixed z-30 w-72 rounded-menu border border-light-border-strong bg-light-surface p-1.5 shadow-menu dark:border-dark-border-strong dark:bg-dark-surface"
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
                    className="flex h-8 w-full cursor-pointer items-center gap-3 whitespace-nowrap rounded-lg px-2.5 text-left text-row text-light-text hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border disabled:cursor-not-allowed disabled:opacity-50 dark:text-dark-text dark:hover:bg-dark-hover"
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
            </div>,
            document.body,
          )
        : null}
    </div>
  )
}
