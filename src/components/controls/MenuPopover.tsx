import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type Ref,
  type RefObject,
} from 'react'
import { createPortal } from 'react-dom'
import { CheckIcon } from '@/components/icons'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { handleKeyByTable } from '@/lib/key-table'
import { wrapIndex } from '@/lib/numbers'
import {
  applyMenuGeometry,
  documentZoom,
  placePopoverAtTrigger,
  toZoomedGeometry,
} from '@/components/menus/menu-geometry'
import { FLYOUT_SURFACE, HOVER_AND_PRESS_ON_POINTER, PRIMARY_TEXT } from '@/lib/class-names'

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

const MENU_OPENING_KEYS = ['ArrowDown', 'ArrowUp', 'Home', 'End']

function useMenuPlacement(
  open: boolean,
  menuRef: RefObject<HTMLDivElement | null>,
  triggerRef: RefObject<HTMLButtonElement | null>,
) {
  useLayoutEffect(() => {
    const menu = menuRef.current
    const trigger = triggerRef.current
    if (!open) return
    if (!menu) return
    if (!trigger) return

    const geometry = placePopoverAtTrigger(
      trigger.getBoundingClientRect(),
      menu.getBoundingClientRect(),
    )
    applyMenuGeometry(menu, toZoomedGeometry(geometry, documentZoom()))
  }, [open, menuRef, triggerRef])
}

function initialFocusIndex(items: MenuPopoverItem[], radio: boolean, enabledIndexes: number[]) {
  const checkedIndex = radio
    ? (items as MenuPopoverRadioItem[]).findIndex((item) => item.checked)
    : -1
  return checkedIndex >= 0 ? checkedIndex : (enabledIndexes[0] ?? 0)
}

type MenuFocusControls = {
  itemRefs: RefObject<Array<HTMLButtonElement | null>>
  enabledIndexes: () => number[]
  focusItem: (index: number) => void
  closeAndRestoreFocus: () => void
}

function handleMenuKeyDown(event: ReactKeyboardEvent<HTMLDivElement>, controls: MenuFocusControls) {
  if (event.key === 'Tab') {
    controls.closeAndRestoreFocus()
    return
  }
  const currentIndex = controls.itemRefs.current.findIndex(
    (item) => item === document.activeElement,
  )
  const indexes = controls.enabledIndexes()
  const currentPosition = indexes.indexOf(currentIndex)
  const focusEnabledAt = (position: number) =>
    controls.focusItem(indexes[wrapIndex(position, indexes.length)] ?? 0)
  handleKeyByTable(event, {
    Escape: () => controls.closeAndRestoreFocus(),
    ArrowDown: () => focusEnabledAt(currentPosition + 1),
    ArrowUp: () => focusEnabledAt(currentPosition - 1),
    Home: () => focusEnabledAt(0),
    End: () => focusEnabledAt(-1),
  })
}

function useMenuPopoverState(items: MenuPopoverItem[], radio: boolean) {
  const [open, setOpen] = useState(false)
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
    requestAnimationFrame(() => focusItem(initialFocusIndex(items, radio, enabledIndexes())))
  }

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node
      const insideMenu = menuRef.current?.contains(target) === true
      const insideTrigger = triggerRef.current?.contains(target) === true
      const outside = !insideMenu && !insideTrigger
      if (outside) closeAndRestoreFocus()
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [open])

  useMenuPlacement(open, menuRef, triggerRef)

  const focusControls = { itemRefs, enabledIndexes, focusItem, closeAndRestoreFocus }
  return { open, triggerRef, menuRef, focusControls, openMenu }
}

/**
 * A compact, trigger-anchored menu for toolbar controls. It intentionally owns
 * its keyboard and focus lifecycle so callers only describe actions.
 */
export function MenuPopover({ ariaLabel, trigger, items, radio = false }: MenuPopoverProps) {
  const menuId = useId()
  const { open, triggerRef, menuRef, focusControls, openMenu } = useMenuPopoverState(items, radio)
  const { itemRefs, closeAndRestoreFocus } = focusControls

  function toggleMenu() {
    if (open) {
      closeAndRestoreFocus()
      return
    }
    openMenu()
  }

  function onTriggerKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>) {
    const opensMenu = !open && MENU_OPENING_KEYS.includes(event.key)
    if (opensMenu) {
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
              onKeyDown={(event) => handleMenuKeyDown(event, focusControls)}
              className={`fixed z-30 w-menu p-1 ${FLYOUT_SURFACE}`}
            >
              {items.map((item, index) => (
                <MenuPopoverItemButton
                  key={item.id}
                  item={item}
                  radio={radio}
                  itemRef={(element) => {
                    itemRefs.current[index] = element
                  }}
                  onChosen={closeAndRestoreFocus}
                />
              ))}
            </div>,
            document.body,
          )
        : null}
    </div>
  )
}

type MenuPopoverItemButtonProps = {
  item: MenuPopoverItem
  radio: boolean
  itemRef: (element: HTMLButtonElement | null) => void
  onChosen: () => void
}

function MenuPopoverItemButton({ item, radio, itemRef, onChosen }: MenuPopoverItemButtonProps) {
  const radioItem = item as MenuPopoverRadioItem
  const showsCheck = radio && radioItem.checked
  return (
    <button
      ref={itemRef}
      type="button"
      role={radio ? 'menuitemradio' : 'menuitem'}
      aria-checked={radio ? radioItem.checked : undefined}
      disabled={item.disabled}
      onClick={() => {
        item.onSelect()
        onChosen()
      }}
      className={`flex h-8 w-full cursor-pointer items-center gap-3 whitespace-nowrap rounded-control px-3 text-left text-row ${PRIMARY_TEXT} ${HOVER_AND_PRESS_ON_POINTER} disabled:cursor-not-allowed disabled:text-light-text-disabled dark:disabled:text-dark-text-disabled ${FOCUS_RING_INSET}`}
    >
      <span className="flex size-4 shrink-0 items-center justify-center" aria-hidden="true">
        {showsCheck ? (
          <CheckIcon className="size-3.5 text-light-accent dark:text-dark-accent" />
        ) : (
          item.icon
        )}
      </span>
      {item.label}
    </button>
  )
}
