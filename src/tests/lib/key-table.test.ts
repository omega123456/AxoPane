import { caseInsensitiveLetterKeys, handleKeyByTable, hasPrimaryModifier } from '@/lib/key-table'

function keyEvent(key: string) {
  return { key, preventDefault: vi.fn() }
}

describe('handleKeyByTable', () => {
  it('runs the mapped handler and prevents the default action', () => {
    const onEscape = vi.fn()
    const event = keyEvent('Escape')
    expect(handleKeyByTable(event, { Escape: onEscape })).toBe(true)
    expect(onEscape).toHaveBeenCalledOnce()
    expect(event.preventDefault).toHaveBeenCalledOnce()
  })

  it('leaves unmapped keys alone', () => {
    const event = keyEvent('a')
    expect(handleKeyByTable(event, { Escape: vi.fn() })).toBe(false)
    expect(event.preventDefault).not.toHaveBeenCalled()
  })
})

it('ignores keys inherited from the object prototype', () => {
  const event = keyEvent('toString')
  expect(handleKeyByTable(event, {})).toBe(false)
  expect(event.preventDefault).not.toHaveBeenCalled()
})

describe('caseInsensitiveLetterKeys', () => {
  it('maps each letter under both cases', () => {
    const onReplace = vi.fn()
    const table = caseInsensitiveLetterKeys({ r: onReplace })
    expect(Object.keys(table).sort()).toEqual(['R', 'r'])
    expect(handleKeyByTable(keyEvent('R'), table)).toBe(true)
    expect(onReplace).toHaveBeenCalledOnce()
  })
})

describe('hasPrimaryModifier', () => {
  it('is true when Ctrl or Meta is held', () => {
    expect(hasPrimaryModifier({ ctrlKey: true, metaKey: false })).toBe(true)
    expect(hasPrimaryModifier({ ctrlKey: false, metaKey: true })).toBe(true)
  })

  it('is false without either modifier', () => {
    expect(hasPrimaryModifier({ ctrlKey: false, metaKey: false })).toBe(false)
  })
})
