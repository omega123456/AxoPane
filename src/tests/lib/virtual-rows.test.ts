import { virtualListHeight, virtualRowsOrFallback, visibleRowCount } from '@/lib/virtual-rows'

const ROW_HEIGHT = 10
const FALLBACK_LIMIT = 3

describe('virtualRowsOrFallback', () => {
  it('returns the measured rows once the virtualizer has measured', () => {
    const measuredRows = [{ key: 'measured', index: 4, start: 40 }]
    expect(
      virtualRowsOrFallback({
        measuredRows,
        rowCount: 10,
        fallbackRowLimit: FALLBACK_LIMIT,
        rowStart: (index) => index * ROW_HEIGHT,
        keyPrefix: 'row',
      }),
    ).toBe(measuredRows)
  })

  it('renders a bounded first screen before measurement', () => {
    expect(
      virtualRowsOrFallback({
        measuredRows: [],
        rowCount: 10,
        fallbackRowLimit: FALLBACK_LIMIT,
        rowStart: (index) => index * ROW_HEIGHT,
        keyPrefix: 'row',
      }),
    ).toEqual([
      { key: 'row-0', index: 0, start: 0 },
      { key: 'row-1', index: 1, start: 10 },
      { key: 'row-2', index: 2, start: 20 },
    ])
  })

  it('renders nothing for an empty list', () => {
    expect(
      virtualRowsOrFallback({
        measuredRows: [],
        rowCount: 0,
        fallbackRowLimit: FALLBACK_LIMIT,
        rowStart: (index) => index * ROW_HEIGHT,
        keyPrefix: 'row',
      }),
    ).toEqual([])
  })
})

describe('virtualListHeight', () => {
  it('is zero for an empty list', () => {
    expect(virtualListHeight({ measuredHeight: 50, rowCount: 0, estimated: 20 })).toBe(0)
  })

  it('prefers the measured height and adds the trailing gap', () => {
    expect(virtualListHeight({ measuredHeight: 50, rowCount: 2, estimated: 20, trailing: 4 })).toBe(
      54,
    )
  })

  it('falls back to the estimate while unmeasured', () => {
    expect(virtualListHeight({ measuredHeight: 0, rowCount: 2, estimated: 20 })).toBe(20)
  })
})

describe('visibleRowCount', () => {
  function scrollElementWithHeight(clientHeight: number) {
    const element = document.createElement('div')
    Object.defineProperty(element, 'clientHeight', { value: clientHeight })
    return element
  }

  it('counts the whole rows that fit in the viewport', () => {
    expect(visibleRowCount(scrollElementWithHeight(95), ROW_HEIGHT)).toBe(9)
  })

  it('never reports fewer than one row', () => {
    expect(visibleRowCount(scrollElementWithHeight(4), ROW_HEIGHT)).toBe(1)
    expect(visibleRowCount(null, ROW_HEIGHT)).toBe(1)
  })
})
