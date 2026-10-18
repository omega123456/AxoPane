import { vi } from 'vitest'

/**
 * jsdom has no layout, so `useElementWidth` reads an empty computed width.
 * Reports `width` px for every element (or only those `matches` accepts).
 */
export function mockComputedWidth(
  width: number,
  matches: (element: Element) => boolean = () => true,
) {
  // Re-mocking would otherwise capture the existing spy as the original.
  if (vi.isMockFunction(window.getComputedStyle)) {
    window.getComputedStyle.mockRestore()
  }
  const originalGetComputedStyle = window.getComputedStyle
  return vi
    .spyOn(window, 'getComputedStyle')
    .mockImplementation((element: Element, pseudoElement?: string | null) => {
      const style = originalGetComputedStyle(element, pseudoElement)
      if (matches(element)) {
        Object.defineProperty(style, 'width', { configurable: true, value: `${width}px` })
      }
      return style
    })
}
