import { dispatchByKind, type KindHandlers } from '@/lib/kind-table'

type Shape = { kind: 'circle'; radius: number } | { kind: 'square'; side: number }

const describeShape: KindHandlers<Shape, [unit: string], string> = {
  circle: (shape, unit) => `circle of radius ${shape.radius}${unit}`,
  square: (shape, unit) => `square of side ${shape.side}${unit}`,
}

describe('dispatchByKind', () => {
  it('runs the handler registered for the value kind with the extra arguments', () => {
    expect(dispatchByKind(describeShape, { kind: 'circle', radius: 2 }, 'cm')).toBe(
      'circle of radius 2cm',
    )
    expect(dispatchByKind(describeShape, { kind: 'square', side: 3 }, 'mm')).toBe(
      'square of side 3mm',
    )
  })
})
