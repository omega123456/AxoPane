import { describe, expect, it } from 'vitest'
import { noopContextAction } from '@/lib/context-menu/context-menu-actions'
import type { ContextMenuDocument } from '@/lib/types/context-menu'
import {
  activationFor,
  CLOSED_FOCUS,
  closedSubmenuFocus,
  edgeFocus,
  hoveredFocus,
  initialFocus,
  movedFocus,
  openedSubmenuFocus,
  type MenuFocusState,
} from '@/stores/context-menu/menu-focus'

const noop = noopContextAction('noop')

function menuDocument(): ContextMenuDocument {
  return {
    paneId: 'left',
    x: 0,
    y: 0,
    title: 'Menu',
    topStrip: [
      { id: 'cut', label: 'Cut', owner: 'app', action: noop },
      { id: 'copy', label: 'Copy', owner: 'app', action: noop, disabled: true },
    ],
    sections: [
      {
        id: 'main',
        rows: [
          { id: 'open', label: 'Open', owner: 'app', kind: 'action', action: noop },
          {
            id: 'more',
            label: 'More',
            owner: 'app',
            kind: 'submenu',
            children: {
              id: 'more-panel',
              rows: [
                { id: 'child-a', label: 'A', owner: 'app', action: noop, disabled: true },
                { id: 'child-b', label: 'B', owner: 'app', action: noop },
                { id: 'child-c', label: 'C', owner: 'app', action: noop },
              ],
            },
          },
          {
            id: 'locked',
            label: 'Locked',
            owner: 'app',
            kind: 'submenu',
            disabled: true,
            children: { id: 'locked-panel', rows: [] },
          },
        ],
      },
    ],
  }
}

function focusState(overrides: Partial<MenuFocusState> = {}): MenuFocusState {
  return { menu: menuDocument(), ...CLOSED_FOCUS, ...overrides }
}

describe('menu-focus', () => {
  it('starts on the first enabled top-level item', () => {
    expect(initialFocus(menuDocument())).toEqual({ ...CLOSED_FOCUS, activeItemId: 'cut' })
  })

  it('ignores every transition while no menu is open', () => {
    const closed = focusState({ menu: null })
    expect(hoveredFocus(closed, 'cut')).toBeNull()
    expect(movedFocus(closed, 1)).toBeNull()
    expect(edgeFocus(closed, 'end')).toBeNull()
    expect(openedSubmenuFocus(closed, 'more')).toBeNull()
    expect(closedSubmenuFocus(closed)).toBeNull()
  })

  it('hovers plain items, submenu parents and submenu children', () => {
    expect(hoveredFocus(focusState(), 'open')).toEqual({ ...CLOSED_FOCUS, activeItemId: 'open' })
    expect(hoveredFocus(focusState(), 'more')).toEqual({
      activeItemId: 'more',
      openSubmenuId: 'more',
      activeSubmenuItemId: 'child-b',
    })
    expect(hoveredFocus(focusState(), 'child-c')).toEqual({
      activeItemId: 'child-c',
      openSubmenuId: 'more',
      activeSubmenuItemId: 'child-c',
    })
    expect(hoveredFocus(focusState(), 'copy')).toBeNull()
    expect(hoveredFocus(focusState(), 'missing')).toBeNull()
  })

  it('moves through enabled items, wrapping and entering from either end', () => {
    expect(movedFocus(focusState({ activeItemId: 'cut' }), 1)?.activeItemId).toBe('open')
    expect(movedFocus(focusState({ activeItemId: 'cut' }), -1)?.activeItemId).toBe('more')
    expect(movedFocus(focusState(), 1)?.activeItemId).toBe('cut')
    expect(movedFocus(focusState(), -1)?.activeItemId).toBe('more')
    expect(movedFocus(focusState({ activeItemId: 'child-c', openSubmenuId: 'more' }), 1)).toEqual({
      activeItemId: 'child-b',
      openSubmenuId: 'more',
      activeSubmenuItemId: 'child-b',
    })
  })

  it('keeps focus when there is nothing enabled to move to', () => {
    const menu = menuDocument()
    menu.topStrip = []
    menu.sections = []
    expect(movedFocus(focusState({ menu, activeItemId: 'gone' }), 1)?.activeItemId).toBe('gone')
  })

  it('jumps to either edge of the current level', () => {
    expect(edgeFocus(focusState({ activeItemId: 'open' }), 'end')).toEqual({
      ...CLOSED_FOCUS,
      activeItemId: 'more',
    })
    expect(edgeFocus(focusState({ activeItemId: 'child-c' }), 'start')).toEqual({
      activeItemId: 'child-b',
      openSubmenuId: 'more',
      activeSubmenuItemId: 'child-b',
    })
  })

  it('opens a submenu, preserving a remembered child when it is already open', () => {
    expect(openedSubmenuFocus(focusState({ activeItemId: 'more' }))).toEqual({
      activeItemId: 'child-b',
      openSubmenuId: 'more',
      activeSubmenuItemId: 'child-b',
    })
    const reopened = focusState({ openSubmenuId: 'more', activeSubmenuItemId: 'child-c' })
    expect(openedSubmenuFocus(reopened, 'more')?.activeItemId).toBe('child-c')
    expect(openedSubmenuFocus(focusState(), 'open')).toBeNull()
    expect(openedSubmenuFocus(focusState(), 'locked')).toBeNull()
    expect(openedSubmenuFocus(focusState(), 'child-b')).toBeNull()
    expect(openedSubmenuFocus(focusState(), 'missing')).toBeNull()
    expect(openedSubmenuFocus(focusState())).toBeNull()
  })

  it('closes an open submenu back onto its parent', () => {
    expect(closedSubmenuFocus(focusState({ openSubmenuId: 'more' }))).toEqual({
      ...CLOSED_FOCUS,
      activeItemId: 'more',
    })
    expect(closedSubmenuFocus(focusState())).toBeNull()
  })

  it('resolves activation into a dispatch, a submenu focus, or nothing', () => {
    const menu = menuDocument()
    expect(activationFor(menu, 'open')).toEqual({ kind: 'dispatch', paneId: 'left', action: noop })
    expect(activationFor(menu, 'child-b')).toEqual({
      kind: 'dispatch',
      paneId: 'left',
      action: noop,
    })
    expect(activationFor(menu, 'more')).toEqual({
      kind: 'focus',
      focus: { activeItemId: 'child-b', openSubmenuId: 'more', activeSubmenuItemId: 'child-b' },
    })
    expect(activationFor(menu, 'copy')).toBeNull()
    expect(activationFor(menu, 'missing')).toBeNull()
  })
})
