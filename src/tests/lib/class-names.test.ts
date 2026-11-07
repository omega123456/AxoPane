import { dropTargetOutline } from '@/lib/class-names'

describe('dropTargetOutline', () => {
  it('outlines only the current drop target', () => {
    expect(dropTargetOutline(true)).toContain('outline-2')
    expect(dropTargetOutline(false)).toBe('')
  })
})
