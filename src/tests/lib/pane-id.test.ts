import { otherPane } from '@/lib/pane-id'

describe('otherPane', () => {
  it('returns the opposite pane', () => {
    expect(otherPane('left')).toBe('right')
    expect(otherPane('right')).toBe('left')
  })
})
