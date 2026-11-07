import { MENU_VIEWPORT_MARGIN_PX, menuAvailableHeight } from '@/components/menus/menu-geometry'

describe('menuAvailableHeight', () => {
  const originalHeight = window.innerHeight

  afterEach(() => {
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: originalHeight })
  })

  it('leaves the viewport margin above and below the menu', () => {
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 600 })
    expect(menuAvailableHeight()).toBe(600 - MENU_VIEWPORT_MARGIN_PX * 2)
  })

  it('never reports a negative height', () => {
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 4 })
    expect(menuAvailableHeight()).toBe(0)
  })
})
