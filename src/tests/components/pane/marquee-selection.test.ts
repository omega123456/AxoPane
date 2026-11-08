import { describe, expect, it } from 'vitest'
import {
  MARQUEE_DRAG_THRESHOLD_PX,
  detailsEntryIdsInRect,
  marqueeRectBetween,
  marqueeSelection,
} from '@/components/pane/marquee-selection'
import { fileEntry } from '@/tests/stores/panes/pane-fixtures'

const GEOMETRY = { listInsetPx: 0, rowHeightPx: 10, rowCount: 4, parentOffset: 1 }
const ENTRIES = [fileEntry('/a'), fileEntry('/b'), fileEntry('/c')]

describe('marquee selection', () => {
  it('ignores drags below the threshold and normalizes the rectangle', () => {
    const start = { x: 10, y: 10 }
    const nearby = { x: 10 + MARQUEE_DRAG_THRESHOLD_PX - 1, y: 11 }
    expect(marqueeRectBetween(start, nearby)).toBeNull()
    expect(marqueeRectBetween(start, { x: 2, y: 30 })).toEqual({
      left: 2,
      top: 10,
      width: 8,
      height: 20,
    })
  })

  it('maps a details rectangle to entry ids, skipping the parent row', () => {
    const rect = { left: 0, top: 0, width: 5, height: 25 }
    expect(detailsEntryIdsInRect(rect, ENTRIES, GEOMETRY)).toEqual(['/a', '/b'])
    const parentOnly = { left: 0, top: 0, width: 5, height: 5 }
    expect(detailsEntryIdsInRect(parentOnly, ENTRIES, GEOMETRY)).toEqual([])
  })

  it('replaces or extends the selection and focuses the last covered entry', () => {
    const drag = { start: { x: 0, y: 0 }, additive: false, baseSelectedIds: ['/z'] }
    expect(marqueeSelection(drag, ['/a', '/b'])).toEqual({
      selectedIds: ['/a', '/b'],
      anchorId: '/a',
      focusedId: '/b',
    })
    expect(marqueeSelection({ ...drag, additive: true }, [])).toEqual({
      selectedIds: ['/z'],
      anchorId: '/z',
      focusedId: '/z',
    })
    expect(marqueeSelection(drag, [])).toEqual({ selectedIds: [], anchorId: null, focusedId: null })
  })
})
