import { errorMessage, isRejectionWithKind } from '@/lib/error-message'

describe('errorMessage', () => {
  it('reads the message of an Error and stringifies anything else', () => {
    expect(errorMessage(new Error('boom'))).toBe('boom')
    expect(errorMessage('plain')).toBe('plain')
  })
})

describe('isRejectionWithKind', () => {
  it('accepts objects that carry a kind', () => {
    expect(isRejectionWithKind({ kind: 'stale' })).toBe(true)
    expect(isRejectionWithKind({ kind: undefined, message: 'x' })).toBe(true)
  })

  it('rejects null, primitives and objects without a kind', () => {
    expect(isRejectionWithKind(null)).toBe(false)
    expect(isRejectionWithKind('stale')).toBe(false)
    expect(isRejectionWithKind(undefined)).toBe(false)
    expect(isRejectionWithKind({ message: 'failed' })).toBe(false)
  })
})
