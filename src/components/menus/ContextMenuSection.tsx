import { useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { ContextMenuRow } from '@/components/menus/ContextMenuRow'
import {
  isContextMenuSubmenuRow,
  type ContextMenuSubmenuRowItem,
  type ContextMenuSection as ContextMenuSectionModel,
} from '@/lib/types/context-menu'
import {
  documentZoom,
  placeSubmenuBesideAnchor,
  toZoomedGeometry,
} from '@/components/menus/menu-geometry'
import { NO_CLASS, SCROLLBAR_THIN, FLYOUT_SURFACE } from '@/lib/class-names'

type NativeSectionState = {
  loading: boolean
  failed: boolean
  placeholderVisible: boolean
  locked: boolean
}

type SubmenuPosition = { left: number; top: number; height: number; maxHeight: number }

type NativePlaceholder = 'none' | 'hidden' | 'loading' | 'locked'

function nativePlaceholderFor(
  nativeState: NativeSectionState | undefined,
  hasRows: boolean,
): NativePlaceholder {
  if (!nativeState) {
    return 'none'
  }
  if (hasRows) {
    return 'none'
  }
  if (nativeState.loading) {
    return 'loading'
  }
  return nativeState.locked ? 'locked' : 'hidden'
}

function placeholderVisibilityClass(visible: boolean) {
  return visible ? NO_CLASS : 'opacity-0'
}

function NativeLoadingPlaceholder({ visible }: { visible: boolean }) {
  return (
    <div
      aria-hidden={!visible}
      role={visible ? 'status' : undefined}
      aria-label={visible ? 'Loading native menu items' : undefined}
      className={`flex flex-col ${placeholderVisibilityClass(visible)}`}
    >
      {['w-3/5', 'w-4/5', 'w-1/2'].map((width) => (
        <div key={width} className="flex h-8 items-center gap-3 px-3">
          <div className="size-4 rounded-control bg-light-skeleton dark:bg-dark-skeleton" />
          <div
            className={`h-2.5 ${width} rounded-control bg-light-skeleton dark:bg-dark-skeleton`}
          />
        </div>
      ))}
    </div>
  )
}

const nativePlaceholders: Record<NativePlaceholder, (visible: boolean) => ReactNode> = {
  none: () => null,
  hidden: () => null,
  loading: (visible) => <NativeLoadingPlaceholder visible={visible} />,
  locked: () => <div aria-hidden="true" className="min-h-24 px-2" />,
}

function sameSubmenuPosition(current: SubmenuPosition | null, next: SubmenuPosition) {
  return (
    current !== null &&
    current.left === next.left &&
    current.top === next.top &&
    current.height === next.height &&
    current.maxHeight === next.maxHeight
  )
}

function measureSubmenuPosition(anchor: HTMLElement, submenu: HTMLElement): SubmenuPosition {
  submenu.style.height = ''
  submenu.style.maxHeight = ''
  const geometry = placeSubmenuBesideAnchor(
    anchor.getBoundingClientRect(),
    submenu.getBoundingClientRect(),
  )
  const zoomed = toZoomedGeometry(geometry, documentZoom())
  return {
    left: zoomed.left,
    top: zoomed.top,
    height: zoomed.height ?? 0,
    maxHeight: zoomed.maxHeight ?? 0,
  }
}

function submenuStyle(position: SubmenuPosition | null) {
  if (!position) {
    return { visibility: 'hidden' as const }
  }
  return {
    left: `${position.left}px`,
    top: `${position.top}px`,
    height: `${position.height}px`,
    maxHeight: `${position.maxHeight}px`,
  }
}

function ContextSubmenu({
  item,
  position,
  submenuRef,
  activeItemId,
  onHoverItem,
  onActivateItem,
}: {
  item: ContextMenuSubmenuRowItem
  position: SubmenuPosition | null
  submenuRef: RefObject<HTMLDivElement | null>
  activeItemId: string | null
  onHoverItem: (itemId: string) => void
  onActivateItem: (itemId: string) => void
}) {
  return (
    <div
      ref={submenuRef}
      role="menu"
      aria-label={item.label}
      className={`fixed z-10 flex w-menu flex-col overflow-hidden p-1 ${FLYOUT_SURFACE}`}
      style={submenuStyle(position)}
    >
      <div
        data-testid="context-submenu-scroll-body"
        className={`min-h-0 flex-1 overflow-y-auto overscroll-contain ${SCROLLBAR_THIN}`}
      >
        {item.children.rows.map((child) => (
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
  )
}

function useSubmenuPosition(openSubmenuItem: ContextMenuSubmenuRowItem | undefined) {
  const submenuRef = useRef<HTMLDivElement | null>(null)
  const [openSubmenuAnchor, setOpenSubmenuAnchor] = useState<HTMLButtonElement | null>(null)
  const [openSubmenuPosition, setOpenSubmenuPosition] = useState<SubmenuPosition | null>(null)

  useLayoutEffect(() => {
    const submenu = submenuRef.current
    const canPlaceSubmenu =
      openSubmenuAnchor !== null && submenu !== null && openSubmenuItem !== undefined
    if (!canPlaceSubmenu) {
      setOpenSubmenuPosition(null)
      return
    }

    const next = measureSubmenuPosition(openSubmenuAnchor, submenu)
    setOpenSubmenuPosition((current) => (sameSubmenuPosition(current, next) ? current : next))
  }, [openSubmenuAnchor, openSubmenuItem])

  return { submenuRef, setOpenSubmenuAnchor, openSubmenuPosition }
}

function SectionRow({
  item,
  active,
  submenuOpen,
  onSubmenuAnchor,
  onHoverItem,
  onActivateItem,
}: {
  item: ContextMenuSectionModel['rows'][number]
  active: boolean
  submenuOpen: boolean
  onSubmenuAnchor: (anchor: HTMLButtonElement | null) => void
  onHoverItem: (itemId: string) => void
  onActivateItem: (itemId: string) => void
}) {
  return (
    <ContextMenuRow
      item={item}
      active={active}
      submenuOpen={submenuOpen}
      buttonRef={submenuOpen ? onSubmenuAnchor : undefined}
      onPointerEnter={() => onHoverItem(item.id)}
      onActivate={() => onActivateItem(item.id)}
    />
  )
}

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
  nativeState?: NativeSectionState
}) {
  const nativePlaceholder = nativePlaceholderFor(nativeState, section.rows.length > 0)
  const openSubmenuItem = section.rows.find(
    (item): item is ContextMenuSubmenuRowItem =>
      isContextMenuSubmenuRow(item) && item.id === openSubmenuId,
  )
  const { submenuRef, setOpenSubmenuAnchor, openSubmenuPosition } =
    useSubmenuPosition(openSubmenuItem)

  if (nativePlaceholder === 'hidden') {
    return null
  }

  return (
    <div>
      {showDivider ? <div className="my-1 h-px bg-light-divider dark:bg-dark-divider" /> : null}

      <div className="px-1">
        {nativePlaceholders[nativePlaceholder](Boolean(nativeState?.placeholderVisible))}
        {section.rows.map((item) => (
          <SectionRow
            key={item.id}
            item={item}
            active={item.id === activeItemId}
            submenuOpen={isContextMenuSubmenuRow(item) && item.id === openSubmenuId}
            onSubmenuAnchor={setOpenSubmenuAnchor}
            onHoverItem={onHoverItem}
            onActivateItem={onActivateItem}
          />
        ))}
      </div>
      {openSubmenuItem ? (
        <ContextSubmenu
          item={openSubmenuItem}
          position={openSubmenuPosition}
          submenuRef={submenuRef}
          activeItemId={activeItemId}
          onHoverItem={onHoverItem}
          onActivateItem={onActivateItem}
        />
      ) : null}
    </div>
  )
}
