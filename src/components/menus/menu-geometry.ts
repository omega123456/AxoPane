import { BOTH_SIDES } from '@/lib/numbers'

export const MENU_VIEWPORT_MARGIN_PX = 8
export const SUBMENU_GAP_PX = 8

export function menuAvailableHeight() {
  return Math.max(window.innerHeight - MENU_VIEWPORT_MARGIN_PX * BOTH_SIDES, 0)
}
