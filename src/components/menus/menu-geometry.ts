import { BOTH_SIDES, clamp } from '@/lib/numbers'

export const MENU_VIEWPORT_MARGIN_PX = 8
export const SUBMENU_GAP_PX = 8

type Size = { width: number; height: number }

type AnchorRect = Pick<DOMRect, 'left' | 'right' | 'top' | 'bottom'>

export type MenuGeometry = {
  left: number
  top: number
  height?: number
  maxHeight?: number
}

export function documentZoom() {
  return Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
}

export function menuAvailableHeight() {
  return Math.max(window.innerHeight - MENU_VIEWPORT_MARGIN_PX * BOTH_SIDES, 0)
}

export function placeMenuAtPoint(pointX: number, pointY: number, size: Size): MenuGeometry {
  const margin = MENU_VIEWPORT_MARGIN_PX
  const availableHeight = menuAvailableHeight()
  const clampedHeight = Math.min(size.height, availableHeight)
  const maxLeft = window.innerWidth - size.width - margin
  const maxTop = window.innerHeight - clampedHeight - margin
  const left = pointX > maxLeft ? pointX - size.width : pointX
  const top = pointY > maxTop ? pointY - size.height : pointY
  return {
    left: clamp(left, margin, maxLeft),
    top: clamp(top, margin, maxTop),
    height: clampedHeight,
    maxHeight: availableHeight,
  }
}

export function placeSubmenuBesideAnchor(anchor: AnchorRect, size: Size): MenuGeometry {
  const margin = MENU_VIEWPORT_MARGIN_PX
  const availableHeight = menuAvailableHeight()
  const clampedHeight = Math.min(size.height, availableHeight)
  const maxLeft = window.innerWidth - margin - size.width
  const overflowsRight = anchor.right + SUBMENU_GAP_PX + size.width > window.innerWidth - margin
  const left = overflowsRight
    ? anchor.left - size.width - SUBMENU_GAP_PX
    : anchor.right + SUBMENU_GAP_PX
  const maxTop = window.innerHeight - margin - clampedHeight
  return {
    left: clamp(left, margin, maxLeft),
    top: clamp(anchor.top, margin, maxTop),
    height: clampedHeight,
    maxHeight: availableHeight,
  }
}

export function placePopoverAtTrigger(trigger: AnchorRect, size: Size): MenuGeometry {
  const margin = MENU_VIEWPORT_MARGIN_PX
  const fitsBelow = trigger.bottom + size.height <= window.innerHeight - margin
  const top = fitsBelow ? trigger.bottom : trigger.top - size.height
  return {
    left: clamp(trigger.left, margin, window.innerWidth - size.width - margin),
    top: Math.max(margin, top),
  }
}

export function toZoomedGeometry(geometry: MenuGeometry, zoom: number): MenuGeometry {
  return {
    left: geometry.left / zoom,
    top: geometry.top / zoom,
    height: geometry.height === undefined ? undefined : geometry.height / zoom,
    maxHeight: geometry.maxHeight === undefined ? undefined : geometry.maxHeight / zoom,
  }
}

export function applyMenuGeometry(node: HTMLElement, geometry: MenuGeometry) {
  node.style.left = `${geometry.left}px`
  node.style.top = `${geometry.top}px`
  if (geometry.height !== undefined) node.style.height = `${geometry.height}px`
  if (geometry.maxHeight !== undefined) node.style.maxHeight = `${geometry.maxHeight}px`
}
