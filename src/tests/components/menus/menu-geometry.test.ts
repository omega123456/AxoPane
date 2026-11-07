import {
  MENU_VIEWPORT_MARGIN_PX,
  SUBMENU_GAP_PX,
  applyMenuGeometry,
  documentZoom,
  menuAvailableHeight,
  placeMenuAtPoint,
  placePopoverAtTrigger,
  placeSubmenuBesideAnchor,
  toZoomedGeometry,
} from '@/components/menus/menu-geometry'

const VIEWPORT_WIDTH = 800
const VIEWPORT_HEIGHT = 600

function setViewport(width: number, height: number) {
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: width })
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: height })
}

describe('menu geometry', () => {
  const originalWidth = window.innerWidth
  const originalHeight = window.innerHeight

  beforeEach(() => setViewport(VIEWPORT_WIDTH, VIEWPORT_HEIGHT))

  afterEach(() => {
    setViewport(originalWidth, originalHeight)
    document.documentElement.style.removeProperty('zoom')
  })

  it('leaves the viewport margin above and below the menu', () => {
    expect(menuAvailableHeight()).toBe(VIEWPORT_HEIGHT - MENU_VIEWPORT_MARGIN_PX * 2)
  })

  it('never reports a negative available height', () => {
    setViewport(VIEWPORT_WIDTH, 4)
    expect(menuAvailableHeight()).toBe(0)
  })

  it('anchors a menu at the point when it fits', () => {
    expect(placeMenuAtPoint(100, 120, { width: 200, height: 300 })).toEqual({
      left: 100,
      top: 120,
      height: 300,
      maxHeight: VIEWPORT_HEIGHT - MENU_VIEWPORT_MARGIN_PX * 2,
    })
  })

  it('flips a menu around the point near the bottom-right corner', () => {
    const geometry = placeMenuAtPoint(750, 550, { width: 200, height: 300 })
    expect(geometry.left).toBe(550)
    expect(geometry.top).toBe(250)
  })

  it('opens a submenu to the anchor right, or left when it would overflow', () => {
    const size = { width: 200, height: 100 }
    expect(
      placeSubmenuBesideAnchor({ left: 100, right: 300, top: 50, bottom: 80 }, size).left,
    ).toBe(300 + SUBMENU_GAP_PX)
    expect(
      placeSubmenuBesideAnchor({ left: 500, right: 700, top: 50, bottom: 80 }, size).left,
    ).toBe(500 - 200 - SUBMENU_GAP_PX)
  })

  it('drops a popover below its trigger, or above when there is no room', () => {
    const size = { width: 100, height: 200 }
    expect(placePopoverAtTrigger({ left: 10, right: 60, top: 40, bottom: 60 }, size)).toEqual({
      left: MENU_VIEWPORT_MARGIN_PX + 2,
      top: 60,
    })
    expect(placePopoverAtTrigger({ left: 10, right: 60, top: 500, bottom: 520 }, size).top).toBe(
      300,
    )
  })

  it('divides geometry by the document zoom and writes it to the node', () => {
    document.documentElement.style.setProperty('zoom', '2')
    const node = document.createElement('div')
    applyMenuGeometry(node, toZoomedGeometry({ left: 40, top: 20, height: 100 }, 2))
    expect(node.style.left).toBe('20px')
    expect(node.style.top).toBe('10px')
    expect(node.style.height).toBe('50px')
    expect(node.style.maxHeight).toBe('')
    expect(documentZoom()).toBeGreaterThan(0)
  })
})
